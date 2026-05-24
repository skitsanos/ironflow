--[[
Index a VTT/SRT transcript into S3 Vectors (ingest only).

This is the "just index my transcripts" recipe: extract -> chunk -> embed ->
store, with per-chunk metadata so retrieved vectors carry their source back.
Unlike s3vector_rag_ingest_query.lua (a full ingest + query + cleanup round
trip), this flow stops after upserting vectors so you can query them later
from a separate flow or service.

Reusable via context — override any input with --context:
  ironflow run examples/16-s3vector/s3vector_transcript_index.lua --context '{
    "transcript_path": "data/samples/interview_long.vtt",
    "bucket_name": "my-transcripts",
    "index_name": "my-transcripts-index"
  }'

Requirements:
- OPENAI_API_KEY (for ai_embed)
- AWS credentials + AWS_REGION (for S3 Vectors)

Notes:
- extract_vtt handles WebVTT; for .srt use nodes.extract_srt (same output shape).
- Embedding dimension below (1536) matches text-embedding-3-small. Change both
  the model and the index `dimension` together if you swap models.
]]

local flow = Flow.new("s3vector_transcript_index")

--[[ Step 0: resolve inputs with safe fallbacks so the flow runs as-is. ]]
flow:step("inputs", nodes.code({
    source = function(ctx)
        local path = ctx.transcript_path
        if type(path) ~= "string" or path == "" then
            path = "data/samples/interview_long.vtt"
        end
        local suffix = now_unix_ms()
        return {
            transcript_path = path,
            bucket_name = ctx.bucket_name or ("ironflow-transcripts-" .. suffix),
            index_name = ctx.index_name or ("ironflow-transcripts-index-" .. suffix)
        }
    end
}))

--[[ Step 1: create the vector bucket and index (1536-dim for 3-small). ]]
flow:step("create_bucket", nodes.s3vector_create_bucket({
    vector_bucket_name = "${ctx.bucket_name}",
    output_key = "bucket"
})):depends_on("inputs")

flow:step("create_index", nodes.s3vector_create_index({
    vector_bucket_name = "${ctx.bucket_name}",
    index_name = "${ctx.index_name}",
    data_type = "float32",
    distance_metric = "cosine",
    dimension = 1536,
    output_key = "index"
})):depends_on("create_bucket")

--[[ Step 2: extract transcript text from the VTT file. ]]
flow:step("extract", nodes.extract_vtt({
    path = "${ctx.transcript_path}",
    format = "text",
    output_key = "transcript"
})):depends_on("create_index")

--[[ Step 3: fixed-size chunks with sentence/newline-aware boundaries. ]]
flow:step("chunk", nodes.ai_chunk({
    mode = "fixed",
    source_key = "transcript",
    output_key = "raw_chunks",
    size = 1200,
    delimiters = "\n."
})):depends_on("extract")

--[[ Step 4: trim and drop empty chunks before embedding. ]]
flow:step("prepare_chunks", nodes.foreach({
    source_key = "raw_chunks",
    output_key = "chunk_texts",
    transform = function(chunk)
        local text = (chunk or ""):gsub("^%s+", ""):gsub("%s+$", "")
        if text == "" then
            return nil
        end
        return text
    end
})):depends_on("chunk")

--[[ Step 5: embed each chunk with OpenAI. ]]
flow:step("embed", nodes.ai_embed({
    provider = "openai",
    model = "text-embedding-3-small",
    input_key = "chunk_texts",
    output_key = "chunk_vectors"
})):depends_on("prepare_chunks")

--[[ Step 6: pair chunks with embeddings into vector payloads + metadata. ]]
flow:step("build_vectors", nodes.code({
    source = function(ctx)
        local vectors = {}
        local texts = ctx.chunk_texts or {}
        local embeddings = ctx.chunk_vectors_embeddings or {}
        local source_file = (ctx.transcript_path or ""):match("([^/]+)$") or ctx.transcript_path

        local limit = #texts
        if #embeddings < limit then
            limit = #embeddings
        end

        for i = 1, limit do
            local vector = embeddings[i]
            if type(vector) == "table" then
                table.insert(vectors, {
                    key = string.format("transcript-chunk-%03d", i),
                    data = vector,
                    metadata = {
                        source_file = source_file,
                        chunk_index = i,
                        char_count = #texts[i],
                        kind = "transcript"
                    }
                })
            end
        end

        return {
            vectors = vectors,
            vector_count = #vectors
        }
    end
})):depends_on("embed")

--[[ Step 7: upsert the vectors for later retrieval. ]]
flow:step("put_vectors", nodes.s3vector_put_vectors({
    vector_bucket_name = "${ctx.bucket_name}",
    index_name = "${ctx.index_name}",
    vectors_source_key = "vectors",
    output_key = "store"
})):depends_on("build_vectors")

--[[ Step 8: report what was indexed. ]]
flow:step("log_result", nodes.log({
    message = "Indexed ${ctx.vector_count} chunk vectors from ${ctx.transcript_path} into ${ctx.bucket_name}/${ctx.index_name}"
})):depends_on("put_vectors")

return flow
