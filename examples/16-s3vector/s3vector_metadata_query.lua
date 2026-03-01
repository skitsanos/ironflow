--[[
Store VTT chunk embeddings with metadata and query using a metadata filter.

Flow:
1) Generate a deterministic bucket/index name for temporary storage.
2) Create the S3 Vector bucket and index.
3) Extract a VTT transcript and split it into chunks.
4) Convert each chunk into a vector and attach metadata.
5) Store the vectors in S3 Vectors.
6) Query the index with a metadata filter and inspect returned metadata.
7) Delete inserted vectors to keep the namespace clean.

Prerequisites:
- OPENAI_API_KEY for embeddings.
- AWS credentials and region configured.
]]

local flow = Flow.new("s3vector_metadata_query")

--[[ Step 1: create deterministic names ]]
flow:step("naming", nodes.code({
    source = function()
        local suffix = now_unix_ms()
        return {
            bucket_name = "ironflow-meta-" .. suffix,
            index_name = "ironflow-meta-index-" .. suffix,
        }
    end
}))

--[[ Step 2: create vector bucket and index ]]
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

--[[ Step 3: extract interview transcript and split into chunks ]]
flow:step("extract_vtt", nodes.extract_vtt({
    path = "data/samples/interview_long.vtt",
    format = "text",
    output_key = "transcript"
})):depends_on("create_index")

flow:step("split_chunks", nodes.ai_chunk({
    mode = "fixed",
    source_key = "transcript",
    output_key = "candidate_chunks",
    size = 1200,
    delimiters = "\n."
})):depends_on("extract_vtt")

flow:step("clean_chunks", nodes.foreach({
    source_key = "candidate_chunks",
    output_key = "chunk_texts",
    transform = function(chunk)
        local text = (chunk or ""):gsub("^%s+", ""):gsub("%s+$", "")
        if text == "" then
            return nil
        end
        return text
    end
})):depends_on("split_chunks")

--[[ Step 4: embed chunks with metadata-friendly document context ]]
flow:step("embed_chunks", nodes.ai_embed({
    provider = "openai",
    model = "text-embedding-3-small",
    input_key = "chunk_texts",
    output_key = "chunk_vectors"
})):depends_on("clean_chunks")

--[[ Step 5: create vector payloads including metadata ]]
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
            local embedding = embeddings[i]
            local text = texts[i]
            if type(embedding) == "table" then
                local key = string.format("interview-%03d", i)
                table.insert(vector_keys, key)
                table.insert(vectors, {
                    key = key,
                    data = embedding,
                    metadata = {
                        source_file = "interview_long.vtt",
                        chunk_index = i,
                        char_count = #text,
                    }
                })
            end
        end

        return {
            vectors = vectors,
            vector_keys = vector_keys,
            chunk_count = #vector_keys
        }
    end
})):depends_on("embed_chunks")

--[[ Step 6: store vectors with metadata ]]
flow:step("upsert_vectors", nodes.s3vector_put_vectors({
    vector_bucket_name = "${ctx.bucket_name}",
    index_name = "${ctx.index_name}",
    vectors_source_key = "vectors",
    output_key = "stored"
})):depends_on("build_vectors")

--[[ Step 7: generate a query embedding from natural language text ]]
flow:step("query_text", nodes.code({
    source = function()
        return {
            query_prompt = "What is this conversation about in one concise sentence?"
        }
    end
})):depends_on("upsert_vectors")

flow:step("query_embedding", nodes.ai_embed({
    provider = "openai",
    model = "text-embedding-3-small",
    input_key = "query_prompt",
    output_key = "query_embedding"
})):depends_on("query_text")

flow:step("query_vector", nodes.code({
    source = function()
        local vectors = ctx.query_embedding_embeddings or {}
        local first = vectors[1]
        if type(first) == "table" then
            return { query_vector = first }
        end
        return { query_vector = {} }
    end
})):depends_on("query_embedding")

--[[ Step 7: query with metadata filter and return metadata in results ]]
flow:step("query_metadata", nodes.s3vector_query_vectors({
    vector_bucket_name = "${ctx.bucket_name}",
    index_name = "${ctx.index_name}",
    top_k = 3,
    query_vector_key = "query_vector",
    filter = {
        source_file = "interview_long.vtt"
    },
    return_metadata = true,
    return_distance = true,
    output_key = "meta_query"
})):depends_on("query_vector")

--[[ Step 8: log query summary and first key ]]
flow:step("log_result", nodes.log({
    message = "Metadata query returned ${ctx.meta_query_count} vector(s), first hit=${ctx.meta_query_vectors[1].key}"
})):depends_on("query_metadata")

--[[ Step 9: cleanup inserted vectors by key ]]
flow:step("cleanup_vectors", nodes.s3vector_delete_vectors({
    vector_bucket_name = "${ctx.bucket_name}",
    index_name = "${ctx.index_name}",
    keys_source_key = "vector_keys",
    output_key = "cleanup"
})):depends_on("query_metadata")

return flow
