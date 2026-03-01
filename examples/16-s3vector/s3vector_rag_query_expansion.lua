--[[
RAG ingestion + query-expansion search pattern with S3 Vectors.

Flow:
1) Build deterministic bucket/index names for this run.
2) Create bucket and index in S3 Vectors.
3) Extract an interview VTT transcript.
4) Chunk transcript text and normalize chunks.
5) Embed chunks and prepare vector payloads with metadata.
6) Upsert vectors into an index.
7) Start with a base query text.
8) Expand the query using `nodes.llm` (multiple variants).
9) Merge base + expanded variants into one rich retrieval prompt.
10) Embed that expanded search prompt and run semantic query.
11) Log retrieved results and clean up test vectors.

This demonstrates a simple query expansion technique:
- use the LLM to generate paraphrases/alternative formulations,
- concatenate them with the original query,
- retrieve on the expanded string for better recall.

Requirements:
- OPENAI_API_KEY
- AWS credentials for S3 Vector
- AWS_REGION or equivalent AWS_REGION-compatible env var
]]

local flow = Flow.new("s3vector_rag_query_expansion")

--[[ Step 1: create stable names for a temporary index ]]
flow:step("naming", nodes.code({
    source = function()
        local suffix = now_unix_ms()
        return {
            bucket_name = "ironflow-rag-expansion-" .. suffix,
            index_name = "ironflow-rag-expansion-index-" .. suffix
        }
    end
}))

--[[ Step 2: provision bucket ]]
flow:step("create_bucket", nodes.s3vector_create_bucket({
    vector_bucket_name = "${ctx.bucket_name}",
    output_key = "bucket"
})):depends_on("naming")

--[[ Step 3: provision index ]]
flow:step("create_index", nodes.s3vector_create_index({
    vector_bucket_name = "${ctx.bucket_name}",
    index_name = "${ctx.index_name}",
    data_type = "float32",
    distance_metric = "euclidean",
    dimension = 1536,
    output_key = "index"
})):depends_on("create_bucket")

--[[ Step 4: extract interview transcript text ]]
flow:step("extract_vtt", nodes.extract_vtt({
    path = "data/samples/interview_long.vtt",
    format = "text",
    output_key = "transcript"
})):depends_on("create_index")

--[[ Step 5: split transcript into manageable chunks ]]
flow:step("chunk_document", nodes.ai_chunk({
    mode = "fixed",
    source_key = "transcript",
    output_key = "raw_chunks",
    size = 1200,
    delimiters = "\n."
})):depends_on("extract_vtt")

--[[ Step 6: trim whitespace and remove empty chunks ]]
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

--[[ Step 7: embed each chunk for storage ]]
flow:step("embed_chunks", nodes.ai_embed({
    provider = "openai",
    model = "text-embedding-3-small",
    input_key = "chunk_texts",
    output_key = "chunk_vectors"
})):depends_on("prepare_chunks")

--[[ Step 8: create vector payloads with source metadata ]]
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
                local key = string.format("rag-exp-chunk-%03d", i)
                local text = texts[i] or ""
                table.insert(vector_keys, key)
                table.insert(vectors, {
                    key = key,
                    data = vector,
                    metadata = {
                        source_file = "interview_long.vtt",
                        chunk_index = i,
                        source = "vtt",
                        char_count = #text
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

--[[ Step 9: upsert vectors into index ]]
flow:step("put_vectors", nodes.s3vector_put_vectors({
    vector_bucket_name = "${ctx.bucket_name}",
    index_name = "${ctx.index_name}",
    vectors_source_key = "vectors",
    output_key = "store"
})):depends_on("build_vectors")

--[[ Step 10: define base query ]]
flow:step("query_base", nodes.code({
    source = function()
        return {
            query_text = "What are the key benefits of IronFlow discussed in the conversation?"
        }
    end
})):depends_on("put_vectors")

--[[ Step 11: expand the query with LLM-generated paraphrases ]]
flow:step("expand_query", nodes.llm({
    provider = "openai",
    model = "gpt-5-mini",
    mode = "chat",
    prompt = "Rewrite this search query into 3 concise alternative formulations for retrieval. "
           .. "Return strict JSON with one key: expanded_queries (array of exactly 3 short strings). "
           .. "Do not add explanations.\n\n"
           .. "${ctx.query_text}",
    output_key = "query_expansion",
    extra = {
        response_format = {
            type = "json_schema",
            json_schema = {
                name = "query_expansion_schema",
                strict = true,
                schema = {
                    type = "object",
                    properties = {
                        expanded_queries = {
                            type = "array",
                            minItems = 3,
                            maxItems = 3,
                            items = { type = "string" }
                        }
                    },
                    required = { "expanded_queries" },
                    additionalProperties = false
                }
            }
        }
    }
})):depends_on("query_base")

--[[ Step 12: merge base query + expanded variants into a retrieval string ]]
flow:step("compose_expanded_prompt", nodes.code({
    source = function()
        local variants = {}
        local decoded = json_parse(ctx.query_expansion_text or "{}")
        local expanded = decoded.expanded_queries
        if type(expanded) == "table" then
            for _, item in ipairs(expanded) do
                if type(item) == "string" and item:gsub("%s+", "") ~= "" then
                    table.insert(variants, item)
                end
            end
        end

        local unique = {}
        local ordered = { ctx.query_text }
        if #ordered == 1 and ordered[1] then
            -- keep original query first
        end
        for _, item in ipairs(variants) do
            local already_exists = false
            for _, seen in ipairs(ordered) do
                if seen == item then
                    already_exists = true
                    break
                end
            end
            if not already_exists then
                table.insert(ordered, item)
            end
        end

        local expanded_query = table.concat(ordered, "\n")
        unique.expanded_query_count = #ordered
        unique.expanded_query_text = expanded_query
        unique.query_expansions = ordered
        return unique
    end
})):depends_on("expand_query")

--[[ Step 13: embed expanded query text for retrieval ]]
flow:step("query_embedding", nodes.ai_embed({
    provider = "openai",
    model = "text-embedding-3-small",
    input_key = "expanded_query_text",
    output_key = "query_embedding"
})):depends_on("compose_expanded_prompt")

--[[ Step 14: extract embedding vector array from node output ]]
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

--[[ Step 15: query index with expanded retrieval context ]]
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

--[[ Step 16: print summary with query expansion + retrieval ]]
flow:step("log_results", nodes.log({
    message = "Query expansion count=${ctx.expanded_query_count}; query variants=${ctx.query_expansions[1]}, ${ctx.query_expansions[2]}, ${ctx.query_expansions[3]}; results=${ctx.rag_query_count}"
})):depends_on("query_vectors")

--[[ Step 17: cleanup inserted vectors ]]
flow:step("cleanup", nodes.s3vector_delete_vectors({
    vector_bucket_name = "${ctx.bucket_name}",
    index_name = "${ctx.index_name}",
    keys_source_key = "vector_keys",
    output_key = "cleanup"
})):depends_on("query_vectors")

return flow
