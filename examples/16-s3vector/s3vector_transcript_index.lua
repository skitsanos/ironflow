--[[
Index a VTT/SRT transcript into S3 Vectors (ingest only).

This is the "just index my transcripts" recipe: extract -> chunk -> embed ->
store, with per-chunk timecode metadata so retrieved vectors carry their exact
start/end timestamps back. Chunks are time-anchored (ts_start/ts_end are stored
in vector metadata) via ai_chunk mode="cues", which preserves subtitle cue
boundaries while grouping cues into size-bounded segments.

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

--[[ Step 3: group cues into time-anchored chunks (keeps start/end timecodes). ]]
flow:step("chunk", nodes.ai_chunk({
    mode = "cues",
    source_key = "cues",
    output_key = "segments",
    size = 1200
})):depends_on("extract")

--[[ Step 4: embed each chunk's text (parallel array from cues mode). ]]
flow:step("embed", nodes.ai_embed({
    provider = "openai",
    model = "text-embedding-3-small",
    input_key = "segments_texts",
    output_key = "chunk_vectors"
})):depends_on("chunk")

--[[ Step 5: pair embeddings with chunk timecodes into vector payloads. ]]
flow:step("build_vectors", nodes.code({
    source = function(ctx)
        local vectors = {}
        local segments = ctx.segments or {}
        local embeddings = ctx.chunk_vectors_embeddings or {}
        local source_file = (ctx.transcript_path or ""):match("([^/]+)$") or ctx.transcript_path

        local limit = #segments
        if #embeddings < limit then
            limit = #embeddings
        end

        for i = 1, limit do
            local vector = embeddings[i]
            local seg = segments[i]
            if type(vector) == "table" and type(seg) == "table" then
                table.insert(vectors, {
                    key = string.format("transcript-chunk-%03d", i),
                    data = vector,
                    metadata = {
                        source_file = source_file,
                        chunk_index = i,
                        ts_start = seg.ts_start,
                        ts_end = seg.ts_end,
                        start_ms = seg.start_ms,
                        end_ms = seg.end_ms,
                        kind = "transcript"
                    }
                })
            end
        end

        return { vectors = vectors, vector_count = #vectors }
    end
})):depends_on("embed")

--[[ Step 6: upsert the vectors for later retrieval. ]]
flow:step("put_vectors", nodes.s3vector_put_vectors({
    vector_bucket_name = "${ctx.bucket_name}",
    index_name = "${ctx.index_name}",
    vectors_source_key = "vectors",
    output_key = "store"
})):depends_on("build_vectors")

--[[ Step 7: report what was indexed. ]]
flow:step("log_result", nodes.log({
    message = "Indexed ${ctx.vector_count} chunk vectors from ${ctx.transcript_path} into ${ctx.bucket_name}/${ctx.index_name}"
})):depends_on("put_vectors")

return flow
