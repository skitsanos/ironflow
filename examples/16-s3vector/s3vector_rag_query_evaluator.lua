--[[
Compare plain semantic retrieval versus query-expanded retrieval on the same
S3 Vector index and report quality signals for the same user question.

Flow:
1) Create an S3 Vector bucket and index.
2) Extract IronFlow interview transcript from VTT.
3) Chunk and clean transcript text.
4) Create embeddings for all chunks and ingest them with full chunk metadata.
5) Run a baseline query ("benefits of IronFlow") and retrieve top K matches.
6) Expand the query with `nodes.llm` into alternatives.
7) Run retrieval with expanded query and retrieve top K matches.
8) Compute relevance and retrieval metrics for each mode:
   - precision@K
   - recall@K
   - average distance
   - overlap between two methods

Requires:
- OPENAI_API_KEY
- S3_VECTOR credentials (`AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`,
  `AWS_REGION`, `S3_BUCKET`)
]]

local flow = Flow.new("s3vector_rag_query_evaluator")

-- Step 1: set stable names for this run.
flow:step("naming", nodes.code({
    source = function()
        local suffix = now_unix_ms()
        return {
            bucket_name = "ironflow-rag-eval-" .. suffix,
            index_name = "ironflow-rag-eval-index-" .. suffix,
            top_k = 5
        }
    end
}))

-- Step 2: create vector bucket.
flow:step("create_bucket", nodes.s3vector_create_bucket({
    vector_bucket_name = "${ctx.bucket_name}",
    output_key = "bucket"
})):depends_on("naming")

-- Step 3: create vector index.
flow:step("create_index", nodes.s3vector_create_index({
    vector_bucket_name = "${ctx.bucket_name}",
    index_name = "${ctx.index_name}",
    data_type = "float32",
    distance_metric = "euclidean",
    dimension = 1536,
    output_key = "index"
})):depends_on("create_bucket")

-- Step 4: extract transcript.
flow:step("extract_vtt", nodes.extract_vtt({
    path = "data/samples/interview_long.vtt",
    format = "text",
    output_key = "transcript",
    metadata_key = "transcript_metadata"
})):depends_on("create_index")

-- Step 5: split into fixed-size chunks.
flow:step("chunk_document", nodes.ai_chunk({
    mode = "fixed",
    source_key = "transcript",
    output_key = "raw_chunks",
    size = 250,
    delimiters = "\n."
})):depends_on("extract_vtt")

-- Step 6: drop empty chunks.
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

-- Step 7: embed chunk list.
flow:step("embed_chunks", nodes.ai_embed({
    provider = "openai",
    model = "text-embedding-3-small",
    input_key = "chunk_texts",
    output_key = "chunk_vectors"
})):depends_on("prepare_chunks")

-- Step 8: assemble vectors with metadata for post-eval analysis.
flow:step("build_vectors", nodes.code({
    source = function()
        local vectors = {}
        local keys = {}
        local texts = ctx.chunk_texts or {}
        local embeddings = ctx.chunk_vectors_embeddings or {}

        local limit = #texts
        if #embeddings < limit then
            limit = #embeddings
        end

        for i = 1, limit do
            local vec = embeddings[i]
            if type(vec) == "table" then
                local key = string.format("eval-chunk-%03d", i)
                table.insert(vectors, {
                    key = key,
                    data = vec,
                    metadata = {
                        source_file = "interview_long.vtt",
                        chunk_index = i,
                        source = "vtt",
                        char_count = #texts[i],
                        chunk_text = texts[i]
                    }
                })
                table.insert(keys, key)
            end
        end

        return {
            vectors = vectors,
            vector_keys = keys,
            vector_payload_count = #vectors
        }
    end
})):depends_on("embed_chunks")

-- Step 9: upsert vectors.
flow:step("put_vectors", nodes.s3vector_put_vectors({
    vector_bucket_name = "${ctx.bucket_name}",
    index_name = "${ctx.index_name}",
    vectors_source_key = "vectors",
    output_key = "store"
})):depends_on("build_vectors")

-- Step 10: define evaluation query.
flow:step("query_text", nodes.code({
    source = function()
        return {
            query_text = "What are the major benefits of IronFlow discussed in the interview?",
            top_k = ctx.top_k
        }
    end
})):depends_on("put_vectors")

-- Step 11: baseline embedding.
flow:step("query_embedding_base", nodes.ai_embed({
    provider = "openai",
    model = "text-embedding-3-small",
    input_key = "query_text",
    output_key = "base_query_embedding"
})):depends_on("query_text")

-- Step 12: base embedding extraction.
flow:step("query_vector_base", nodes.code({
    source = function()
        local vectors = ctx.base_query_embedding_embeddings or {}
        local first = vectors[1]
        if type(first) == "table" then
            return { query_vector_base = first }
        end
        return { query_vector_base = {} }
    end
})):depends_on("query_embedding_base")

-- Step 13: baseline semantic query.
flow:step("query_vectors_base", nodes.s3vector_query_vectors({
    vector_bucket_name = "${ctx.bucket_name}",
    index_name = "${ctx.index_name}",
    top_k = 4,
    query_vector_key = "query_vector_base",
    filter = { source = "vtt" },
    return_metadata = true,
    return_distance = true,
    output_key = "base_results"
})):depends_on("query_vector_base")

-- Step 14: expand query with LLM alternatives.
flow:step("expand_query", nodes.llm({
    provider = "openai",
    model = "gpt-5-mini",
    mode = "chat",
    prompt = "Rewrite this query into 3 short retrieval-focused alternatives."
          .. "Return JSON only with one key: expanded_queries (array of exactly 3 strings). No explanation.\n\n"
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
})):depends_on("query_text")

-- Step 15: merge base + expanded variants.
flow:step("compose_expanded_query", nodes.code({
    source = function()
        local decoded = json_parse(ctx.query_expansion_text or "{}")
        local expanded = {}
        local candidates = decoded.expanded_queries
        if type(candidates) == "table" then
            for _, item in ipairs(candidates) do
                if type(item) == "string" then
                    local normalized = item:gsub("^%s+", ""):gsub("%s+$", "")
                    if normalized ~= "" then
                        table.insert(expanded, normalized)
                    end
                end
            end
        end

        local ordered = { ctx.query_text }
        for _, item in ipairs(expanded) do
            local exists = false
            for _, seen in ipairs(ordered) do
                if seen == item then
                    exists = true
                    break
                end
            end
            if not exists then
                table.insert(ordered, item)
            end
        end

        return {
            expanded_query_text = table.concat(ordered, "\n"),
            expanded_query_count = #ordered,
            expanded_query_variants = ordered
        }
    end
})):depends_on("expand_query")

-- Step 16: expanded query embedding.
flow:step("query_embedding_expanded", nodes.ai_embed({
    provider = "openai",
    model = "text-embedding-3-small",
    input_key = "expanded_query_text",
    output_key = "expanded_query_embedding"
})):depends_on("compose_expanded_query")

-- Step 17: expanded embedding extraction.
flow:step("query_vector_expanded", nodes.code({
    source = function()
        local vectors = ctx.expanded_query_embedding_embeddings or {}
        local first = vectors[1]
        if type(first) == "table" then
            return { query_vector_expanded = first }
        end
        return { query_vector_expanded = {} }
    end
})):depends_on("query_embedding_expanded")

-- Step 18: expanded semantic query.
flow:step("query_vectors_expanded", nodes.s3vector_query_vectors({
    vector_bucket_name = "${ctx.bucket_name}",
    index_name = "${ctx.index_name}",
    top_k = 4,
    query_vector_key = "query_vector_expanded",
    filter = { source = "vtt" },
    return_metadata = true,
    return_distance = true,
    output_key = "expanded_results"
})):depends_on("query_vector_expanded")

-- Step 19: evaluate both methods against chunk-level relevance.
flow:step("evaluate", nodes.code({
    source = function()
        local function to_set(list)
            local set = {}
            for _, key in ipairs(list or {}) do
                if type(key) == "string" then
                    set[key] = true
                end
            end
            return set
        end

        local function precision_recall(expected_set, result_keys, k)
            local expected_count = 0
            for _ in pairs(expected_set) do
                expected_count = expected_count + 1
            end

            if k == nil then k = 0 end
            local retrieved = 0
            local matched = 0
            local hits = {}

            for i = 1, math.min(k, #result_keys) do
                local key = result_keys[i]
                retrieved = retrieved + 1
                if type(key) == "string" and expected_set[key] then
                    matched = matched + 1
                    table.insert(hits, key)
                end
            end

            local precision = 0
            if retrieved > 0 then
                precision = matched / retrieved
            end

            local recall = 0
            if expected_count > 0 then
                recall = matched / expected_count
            end

            return {
                precision = precision,
                recall = recall,
                matched = matched,
                retrieved = retrieved,
                hit_keys = hits
            }
        end

        local function avg_distance(results)
            local values = {}
            if type(results) == "table" then
                for _, item in ipairs(results) do
                    local d = item.distance
                    if type(d) == "number" then
                        table.insert(values, d)
                    end
                end
            end
            if #values == 0 then
                return 0, 0
            end
            local total = 0
            for _, value in ipairs(values) do
                total = total + value
            end
            return total / #values, #values
        end

        local function gather_expected_keys()
            local relevance = {}
            local terms = {
                "benefit", "benefits", "advantage", "advantages",
                "efficiency", "automation", "quality", "ironflow", "project",
                "workflow", "speed", "scale", "collaboration"
            }

            local source = ctx.chunk_texts or {}
            for i = 1, #source do
                local text = source[i] or ""
                local lower = string.lower(text)
                local hit = false
                for _, term in ipairs(terms) do
                    if lower:find(term, 1, true) then
                        hit = true
                        break
                    end
                end
                if hit then
                    table.insert(relevance, string.format("eval-chunk-%03d", i))
                end
            end

            -- Fallback: use top 2 if retrieval is impossible (keeps evaluator stable).
            if #relevance == 0 then
                relevance = { "eval-chunk-001", "eval-chunk-002" }
            end

            return relevance
        end

        local function overlaps(set_a, set_b)
            local overlap = 0
            for key in pairs(set_a) do
                if set_b[key] then
                    overlap = overlap + 1
                end
            end
            return overlap
        end

        local base_vectors = ctx.base_results_vectors or {}
        local expanded_vectors = ctx.expanded_results_vectors or {}
        local k = ctx.top_k or 4

        local base_keys = {}
        for i = 1, k do
            local item = base_vectors[i]
            if type(item) == "table" and type(item.key) == "string" then
                table.insert(base_keys, item.key)
            end
        end

        local expanded_keys = {}
        for i = 1, k do
            local item = expanded_vectors[i]
            if type(item) == "table" and type(item.key) == "string" then
                table.insert(expanded_keys, item.key)
            end
        end

        local expected = gather_expected_keys()
        local expected_set = to_set(expected)

        local baseline = precision_recall(expected_set, base_keys, k)
        local expanded = precision_recall(expected_set, expanded_keys, k)
        local base_set = to_set(base_keys)
        local expanded_set = to_set(expanded_keys)

        local overlap_count = overlaps(base_set, expanded_set)
        local union_count = 0
        for key in pairs(base_set) do
            union_count = union_count + 1
        end
        for key in pairs(expanded_set) do
            if not base_set[key] then
                union_count = union_count + 1
            end
        end
        local jaccard = 0
        if union_count > 0 then
            jaccard = overlap_count / union_count
        end

        local base_avg_distance, base_returned = avg_distance(base_vectors)
        local expanded_avg_distance, expanded_returned = avg_distance(expanded_vectors)

        return {
            expected_chunks = expected,
            expected_count = #expected,
            base_result_keys = base_keys,
            expanded_result_keys = expanded_keys,
            expanded_query_count = ctx.expanded_query_count,
            expanded_query_variants = ctx.expanded_query_variants,
            base_precision = baseline.precision,
            base_recall = baseline.recall,
            base_matched = baseline.matched,
            base_retrieved = baseline.retrieved,
            expanded_precision = expanded.precision,
            expanded_recall = expanded.recall,
            expanded_matched = expanded.matched,
            expanded_retrieved = expanded.retrieved,
            base_avg_distance = base_avg_distance,
            expanded_avg_distance = expanded_avg_distance,
            base_returned = base_returned,
            expanded_returned = expanded_returned,
            overlap_k = overlap_count,
            union_k = union_count,
            jaccard_k = jaccard
        }
    end
})):depends_on("query_vectors_expanded")

-- Step 20: print a compact comparison.
flow:step("report", nodes.log({
    message = "Expected chunks=${ctx.expected_count}. "
        .. "Base@${ctx.top_k}: P=${ctx.base_precision}, R=${ctx.base_recall}, "
        .. "AvgDist=${ctx.base_avg_distance}; "
        .. "Expanded@${ctx.top_k}: P=${ctx.expanded_precision}, R=${ctx.expanded_recall}, "
        .. "AvgDist=${ctx.expanded_avg_distance}; "
        .. "Overlap=${ctx.overlap_k}/${ctx.union_k}, Jaccard=${ctx.jaccard_k}"
})):depends_on("evaluate")

-- Step 21: keep expected top-k keys visible in logs.
flow:step("report_results", nodes.log({
    message = "Base keys=${ctx.base_result_keys[1]}, ${ctx.base_result_keys[2]}, ${ctx.base_result_keys[3]}, ${ctx.base_result_keys[4]}; "
        .. "Expanded keys=${ctx.expanded_result_keys[1]}, ${ctx.expanded_result_keys[2]}, ${ctx.expanded_result_keys[3]}, ${ctx.expanded_result_keys[4]}"
})):depends_on("report")

-- Step 22: cleanup vectors and index state.
flow:step("cleanup", nodes.s3vector_delete_vectors({
    vector_bucket_name = "${ctx.bucket_name}",
    index_name = "${ctx.index_name}",
    keys_source_key = "vector_keys",
    output_key = "cleanup"
})):depends_on("evaluate")

return flow
