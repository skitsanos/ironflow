--[[
End-to-end RAG ingestion and query pattern with S3 Vectors.

Flow:
1) Build deterministic bucket/index names.
2) Create bucket and index.
3) Extract a VTT transcript.
4) Chunk transcript into fixed-size chunks.
5) Normalize chunks and remove empty items.
6) Embed chunks with OpenAI embeddings.
7) Build vector payloads with chunk metadata and upload them.
8) Embed a user query text.
9) Query vectors by metadata + semantic similarity.
10) Log top results and clean up test vectors.

This is the full “chunk → embed → store → query” sequence for retrieval workflows.

Requirements:
- OPENAI_API_KEY
- AWS credentials for S3 Vector
- AWS_REGION or equivalent AWS_REGION-compatible env var
]]

local flow = Flow.new("s3vector_rag_ingest_query")

--[[ Step 1: generate stable names for this run ]]
flow:step("naming", nodes.code({
    source = function()
        local suffix = now_unix_ms()
        return {
            bucket_name = "ironflow-rag-" .. suffix,
            index_name = "ironflow-rag-index-" .. suffix
        }
    end
}))

--[[ Step 2: create bucket and index first ]]
flow:step("create_bucket", nodes.s3vector_create_bucket({
    vector_bucket_name = "${ctx.bucket_name}",
    output_key = "bucket"
})):depends_on("naming")

flow:step("create_index", nodes.s3vector_create_index({
    vector_bucket_name = "${ctx.bucket_name}",
    index_name = "${ctx.index_name}",
    data_type = "float32",
    distance_metric = "euclidean",
    dimension = 1536,
    output_key = "index"
})):depends_on("create_bucket")

--[[ Step 3: extract transcript content ]]
flow:step("extract_vtt", nodes.extract_vtt({
    path = "data/samples/interview.vtt",
    format = "text",
    output_key = "transcript"
})):depends_on("create_index")

--[[ Step 4: split into chunks suitable for embedding ]]
flow:step("chunk_document", nodes.ai_chunk({
    mode = "fixed",
    source_key = "transcript",
    output_key = "raw_chunks",
    size = 1200,
    delimiters = "\n."
})):depends_on("extract_vtt")

--[[ Step 5: clean chunk text and keep only usable chunks ]]
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
})):depends_on("chunk_document")

--[[ Step 6: generate embeddings for each chunk ]]
flow:step("embed_chunks", nodes.ai_embed({
    provider = "openai",
    model = "text-embedding-3-small",
    input_key = "chunk_texts",
    output_key = "chunk_vectors"
})):depends_on("prepare_chunks")

--[[ Step 7: map chunks + embeddings into S3 Vector payload objects ]]
flow:step("build_vectors", nodes.code({
    source = function()
        local vectors = {}
        local vector_keys = {}
        local texts = ctx.chunk_texts or {}
        local embeddings = ctx.chunk_vectors_embeddings or {}

        local limit = #texts
        if #embeddings < limit then
            limit = #embeddings
        end

        for i = 1, limit do
            local vector = embeddings[i]
            if type(vector) == "table" then
                local key = string.format("rag-chunk-%03d", i)
                table.insert(vector_keys, key)
                table.insert(vectors, {
                    key = key,
                    data = vector,
                    metadata = {
                        source_file = "interview.vtt",
                        chunk_index = i,
                        source = "vtt",
                        char_count = #texts[i]
                    }
                })
            end
        end

        return {
            vectors = vectors,
            vector_keys = vector_keys,
            vector_payload_count = #vector_keys
        }
    end
})):depends_on("embed_chunks")

--[[ Step 8: upsert vectors for indexing ]]
flow:step("put_vectors", nodes.s3vector_put_vectors({
    vector_bucket_name = "${ctx.bucket_name}",
    index_name = "${ctx.index_name}",
    vectors_source_key = "vectors",
    output_key = "store"
})):depends_on("build_vectors")

--[[ Step 9: embed user query ]]
flow:step("query_text", nodes.code({
    source = function()
        return {
            query_text = "What are the key benefits discussed for this project?"
        }
    end
})):depends_on("put_vectors")

flow:step("query_embedding", nodes.ai_embed({
    provider = "openai",
    model = "text-embedding-3-small",
    input_key = "query_text",
    output_key = "query"
})):depends_on("query_text")

flow:step("query_vector", nodes.code({
    source = function()
        local vectors = ctx.query_embeddings or {}
        local first = vectors[1]
        if type(first) == "table" then
            return { query_vector = first }
        end
        return { query_vector = {} }
    end
})):depends_on("query_embedding")

--[[ Step 10: semantic query with metadata filter ]]
flow:step("query_vectors", nodes.s3vector_query_vectors({
    vector_bucket_name = "${ctx.bucket_name}",
    index_name = "${ctx.index_name}",
    top_k = 3,
    query_vector_key = "query_vector",
    filter = {
        source = "vtt"
    },
    return_metadata = true,
    return_distance = true,
    output_key = "rag_query"
})):depends_on("query_vector")

--[[ Step 11: log top result for quick validation ]]
flow:step("log_results", nodes.log({
    message = "RAG query returned ${ctx.rag_query_count} result(s), first=${ctx.rag_query_vectors[1].key}"
})):depends_on("query_vectors")

--[[ Step 12: optional cleanup ]]
flow:step("cleanup", nodes.s3vector_delete_vectors({
    vector_bucket_name = "${ctx.bucket_name}",
    index_name = "${ctx.index_name}",
    keys_source_key = "vector_keys",
    output_key = "cleanup"
})):depends_on("query_vectors")

return flow
