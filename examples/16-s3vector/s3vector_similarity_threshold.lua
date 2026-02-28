--[[
RAG ingestion with client-side cosine similarity filtering.

Flow:
1) Create an S3 Vector bucket and index using cosine metric.
2) Extract a transcript and chunk it for embedding.
3) Generate embeddings and store vectors with metadata.
4) Embed a query and set a minimum cosine similarity threshold.
5) Filter query results by threshold and return matching chunks.
6) Cleanup temporary vectors.

This mirrors the Python research workflow pattern for `S3V_MIN_SIMILARITY`,
implemented at the node level as `min_similarity`.

Requirements:
- OPENAI_API_KEY
- AWS credentials/endpoint for S3 Vectors
- AWS_REGION (or equivalent)
]]

local flow = Flow.new("s3vector_similarity_threshold")

flow:step("naming", nodes.code({
    source = function()
        local suffix = now_unix_ms()
        return {
            bucket_name = "ironflow-sim-" .. suffix,
            index_name = "ironflow-sim-index-" .. suffix
        }
    end
}))

flow:step("create_bucket", nodes.s3vector_create_bucket({
    vector_bucket_name = "${ctx.bucket_name}",
    output_key = "bucket"
})):depends_on("naming")

flow:step("create_index", nodes.s3vector_create_index({
    vector_bucket_name = "${ctx.bucket_name}",
    index_name = "${ctx.index_name}",
    data_type = "float32",
    distance_metric = "cosine",
    dimension = 1536,
    output_key = "index"
})):depends_on("create_bucket")

flow:step("extract_vtt", nodes.extract_vtt({
    path = "data/samples/interview_long.vtt",
    format = "text",
    output_key = "transcript"
})):depends_on("create_index")

flow:step("chunk_document", nodes.ai_chunk({
    mode = "fixed",
    source_key = "transcript",
    output_key = "raw_chunks",
    size = 1200,
    delimiters = "\n."
})):depends_on("extract_vtt")

flow:step("normalize_chunks", nodes.foreach({
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

flow:step("embed_chunks", nodes.ai_embed({
    provider = "openai",
    model = "text-embedding-3-small",
    input_key = "chunk_texts",
    output_key = "chunk_vectors"
})):depends_on("normalize_chunks")

flow:step("build_vectors", nodes.code({
    source = function()
        local vectors = {}
        local keys = {}
        local texts = ctx.chunk_texts or {}
        local embeddings = ctx.chunk_vectors_embeddings or {}

        local total = #texts
        if #embeddings < total then
            total = #embeddings
        end

        for i = 1, total do
            local embedding = embeddings[i]
            if type(embedding) == "table" then
                local key = string.format("sim-%03d", i)
                table.insert(keys, key)
                table.insert(vectors, {
                    key = key,
                    data = embedding,
                    metadata = {
                        source_file = "interview_long.vtt",
                        chunk_index = i
                    }
                })
            end
        end

        return {
            vectors = vectors,
            vector_keys = keys
        }
    end
})):depends_on("embed_chunks")

flow:step("store_vectors", nodes.s3vector_put_vectors({
    vector_bucket_name = "${ctx.bucket_name}",
    index_name = "${ctx.index_name}",
    vectors_source_key = "vectors",
    output_key = "stored"
})):depends_on("build_vectors")

flow:step("query_text", nodes.code({
    source = function()
        return {
            query_text = "What are the concrete benefits of IronFlow for teams building pipelines?"
        }
    end
})):depends_on("store_vectors")

flow:step("query_embedding", nodes.ai_embed({
    provider = "openai",
    model = "text-embedding-3-small",
    input_key = "query_text",
    output_key = "query_embedding"
})):depends_on("query_text")

flow:step("query_vector", nodes.code({
    source = function()
        local first = (ctx.query_embedding_embeddings or {})[1]
        if type(first) == "table" then
            return { query_vector = first }
        end
        return { query_vector = {} }
    end
})):depends_on("query_embedding")

flow:step("query_vectors", nodes.s3vector_query_vectors({
    vector_bucket_name = "${ctx.bucket_name}",
    index_name = "${ctx.index_name}",
    top_k = 10,
    query_vector_key = "query_vector",
    min_similarity = 0.55,
    strict = true,
    return_metadata = true,
    return_distance = true,
    output_key = "similar"
})):depends_on("query_vector")

flow:step("log", nodes.log({
    message = "Filtered results count=${ctx.similar_count}; min_similarity=${ctx.similar_min_similarity}; top=${ctx.similar_vectors[1].key}; distance=${ctx.similar_vectors[1].distance}"
})):depends_on("query_vectors")

flow:step("cleanup", nodes.s3vector_delete_vectors({
    vector_bucket_name = "${ctx.bucket_name}",
    index_name = "${ctx.index_name}",
    keys_source_key = "vector_keys",
    output_key = "cleanup"
})):depends_on("query_vectors")

return flow
