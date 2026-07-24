--[[
S3 Vector workflow example.

Flow:
1) Generate unique names for bucket and index.
2) Create a vector bucket.
3) Inspect the bucket, then create and inspect a float32 index.
4) Upload a tiny vector batch.
5) Query with a sample embedding.
6) Report the query, then delete vectors, index, and bucket in dependency order.

Prerequisites:
- AWS credentials and S3Vectors access.
- AWS_REGION set.
- (Optional) AWS_ENDPOINT_URL for local / custom endpoint.

Effects:
- Creates a UUID-scoped bucket and index. After reporting, successful teardown
  deletes vectors, then the index, then the bucket.
- Cleanup is not a finally block; failure or interruption can retain remote
  resources that may incur provider cost.
]]

local flow = Flow.new("s3vector_vector_workflow")

--[[ Step 1: build unique names in context ]]
flow:step("naming", nodes.code({
    source = function()
        local suffix = uuid4():gsub("-", "")
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

--[[ Step 3: inspect the created bucket ]]
flow:step("get_bucket", nodes.s3vector_get_bucket({
    vector_bucket_name = "${ctx.bucket_name}",
    output_key = "bucket_metadata"
})):depends_on("create_bucket")

--[[ Step 4: create an index inside the bucket ]]
flow:step("create_index", nodes.s3vector_create_index({
    vector_bucket_name = "${ctx.bucket_name}",
    index_name = "${ctx.index_name}",
    data_type = "float32",
    distance_metric = "euclidean",
    dimension = 3,
    output_key = "vector_index"
})):depends_on("get_bucket")

--[[ Step 5: inspect the created index ]]
flow:step("get_index", nodes.s3vector_get_index({
    vector_bucket_name = "${ctx.bucket_name}",
    index_name = "${ctx.index_name}",
    output_key = "index_metadata"
})):depends_on("create_index")

--[[ Step 6: upload example vectors ]]
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
})):depends_on("get_index")

--[[ Step 7: run a nearest-neighbor query ]]
flow:step("query_vectors", nodes.s3vector_query_vectors({
    vector_bucket_name = "${ctx.bucket_name}",
    index_name = "${ctx.index_name}",
    top_k = 2,
    query_vector = { 0.18, 0.31, 0.44 },
    return_metadata = true,
    return_distance = true,
    output_key = "query"
})):depends_on("put_vectors")

--[[ Step 8: log query summary ]]
flow:step("show_query", nodes.log({
    message = "Query vector count: ${ctx.query_count}, top match: ${ctx.query_vectors[0].key}"
})):depends_on("query_vectors")

--[[ Step 9: teardown after reporting: vectors -> index -> bucket ]]
flow:step("delete_vectors", nodes.s3vector_delete_vectors({
    vector_bucket_name = "${ctx.bucket_name}",
    index_name = "${ctx.index_name}",
    keys = { "vector-a", "vector-b" },
    output_key = "deleted_vectors"
})):depends_on("show_query")

flow:step("delete_index", nodes.s3vector_delete_index({
    vector_bucket_name = "${ctx.bucket_name}",
    index_name = "${ctx.index_name}",
    output_key = "deleted_index"
})):depends_on("delete_vectors")

flow:step("delete_bucket", nodes.s3vector_delete_bucket({
    vector_bucket_name = "${ctx.bucket_name}",
    output_key = "deleted_bucket"
})):depends_on("delete_index")

return flow
