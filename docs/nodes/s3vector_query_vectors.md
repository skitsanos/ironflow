# `s3vector_query_vectors`

Run vector similarity search on an Amazon S3 Vector index.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `vector_bucket_name` | string | yes* | `S3VECTOR_BUCKET_NAME` / `S3_BUCKET` env vars | Bucket that owns the index. |
| `bucket` | string | no | no | Alias for `vector_bucket_name`. |
| `vector_bucket_arn` | string | no | `S3VECTOR_BUCKET_ARN` env var | Alternative to `vector_bucket_name`. |
| `index_name` | string | yes* | `S3VECTOR_INDEX_NAME` env var | Target index name. |
| `index` | string | no | no | Alias for `index_name`. |
| `index_arn` | string | no | `S3VECTOR_INDEX_ARN` env var | Alternative to `index_name`. |
| `top_k` | integer | yes | -- | Number of nearest neighbors to return (`> 0`). |
| `query_vector` | array<number> | no* | -- | Query embedding vector. |
| `query_vector_key` | string | no* | -- | Context key containing a query embedding array. |
| `filter` | object | no | -- | Optional metadata filter. |
| `filter_key` | string | no | -- | Context key for a JSON metadata filter object. |
| `return_metadata` | bool | no | `false` | Include vector metadata in results. |
| `return_distance` | bool | no | `false` | Include distance values in results. |
| `output_key` | string | no | `s3vector` | Prefix for context output keys. |

At least one of `query_vector` or `query_vector_key` is required.
`index_name`/`index` require a bucket reference unless `index_arn` is provided.

## Context Output

- `{output_key}_distance_metric` — Similarity metric used.
- `{output_key}_count` — Number of returned result entries.
- `{output_key}_vectors` — Array of result objects (`key`, optional `distance`, optional `metadata`).
- `{output_key}_success` — `true` on success.

## Example

```lua
local flow = Flow.new("s3vector_query_vectors_example")

flow:step("query_vectors", nodes.s3vector_query_vectors({
    vector_bucket_name = "ironflow-vectors-demo",
    index_name = "ironflow-demo-index",
    top_k = 2,
    query_vector = { 0.14, 0.25, 0.31 },
    return_metadata = true,
    return_distance = true,
    output_key = "query"
}))

return flow
```
