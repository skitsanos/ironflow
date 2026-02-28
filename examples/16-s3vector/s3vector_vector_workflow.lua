--[[
S3 Vector workflow example.

Flow:
1) Generate deterministic names for bucket and index.
2) Create a vector bucket.
3) Create an index with float32 vectors.
4) Upload a tiny vector batch.
5) Query with a sample embedding.
6) Delete the inserted vectors to keep storage clean.

Prerequisites:
- AWS credentials and S3Vectors access.
- AWS_REGION set.
- (Optional) AWS_ENDPOINT_URL for local / custom endpoint.
]]

local flow = Flow.new("s3vector_vector_workflow")

--[[ Step 1: build unique names in context ]]
flow:step("naming", nodes.code({
    source = function()
        local suffix = now_unix_ms() % 100000
        local bucket_name = "ironflow-vectors-" .. suffix
        local index_name = "demo-index-" .. suffix
        return {
            bucket_name = bucket_name,
            index_name = index_name
        }
    end
}))

--[[ Step 2: create a bucket ]]
flow:step("create_bucket", nodes.s3vector_create_bucket({
    vector_bucket_name = "${ctx.bucket_name}",
    output_key = "vector_bucket"
})):depends_on("naming")

--[[ Step 3: create an index inside the bucket ]]
flow:step("create_index", nodes.s3vector_create_index({
    vector_bucket_name = "${ctx.bucket_name}",
    index_name = "${ctx.index_name}",
    data_type = "float32",
    distance_metric = "euclidean",
    dimension = 3,
    output_key = "vector_index"
})):depends_on("create_bucket")

--[[ Step 4: upload example vectors ]]
flow:step("put_vectors", nodes.s3vector_put_vectors({
    vector_bucket_name = "${ctx.bucket_name}",
    index_name = "${ctx.index_name}",
    vectors = {
        {
            key = "vector-a",
            data = { 0.15, 0.28, 0.47 },
            metadata = { speaker = "Alex", segment = "opening" }
        },
        {
            key = "vector-b",
            data = { 0.21, 0.45, 0.51 },
            metadata = { speaker = "Mina", segment = "closing" }
        }
    },
    output_key = "vectors"
})):depends_on("create_index")

--[[ Step 5: run a nearest-neighbor query ]]
flow:step("query_vectors", nodes.s3vector_query_vectors({
    vector_bucket_name = "${ctx.bucket_name}",
    index_name = "${ctx.index_name}",
    top_k = 2,
    query_vector = { 0.18, 0.31, 0.44 },
    return_metadata = true,
    return_distance = true,
    output_key = "query"
})):depends_on("put_vectors")

--[[ Step 6: log query summary ]]
flow:step("show_query", nodes.log({
    message = "Query vector count: ${ctx.query_count}, top match: ${ctx.query_vectors[1].key}"
})):depends_on("query_vectors")

--[[ Step 7: cleanup inserted vectors ]]
flow:step("delete_vectors", nodes.s3vector_delete_vectors({
    vector_bucket_name = "${ctx.bucket_name}",
    index_name = "${ctx.index_name}",
    keys = { "vector-a", "vector-b" },
    output_key = "delete"
})):depends_on("query_vectors")

return flow
