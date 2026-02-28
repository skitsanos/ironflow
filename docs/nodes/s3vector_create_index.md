# `s3vector_create_index`

Create an Amazon S3 Vector index inside a bucket.

## Parameters

| Parameter | Type   | Required | Default | Description |
|-----------|--------|----------|---------|-------------|
| `vector_bucket_name` | string | yes* | `S3VECTOR_BUCKET_NAME` / `S3_BUCKET` env vars | Target bucket name. |
| `bucket` | string | no | no | Alias for `vector_bucket_name`. |
| `vector_bucket_arn` | string | no | `S3VECTOR_BUCKET_ARN` env var | Alternative to `vector_bucket_name`. |
| `index_name` | string | yes | `S3VECTOR_INDEX_NAME` env var | Name for the new index. |
| `index` | string | no | no | Alias for `index_name`. |
| `data_type` | string | yes | -- | Vector type (for example: `float32`). |
| `distance_metric` | string | yes | -- | Similarity metric (for example: `euclidean`, `cosine`). |
| `dimension` | integer | yes | -- | Vector dimension (must be > 0). |
| `output_key` | string | no | `s3vector` | Prefix for context output keys. |

## Context Output

- `{output_key}_index_name` — Created index name.
- `{output_key}_bucket_name` — Bucket name used.
- `{output_key}_bucket_arn` — Bucket ARN used if provided.
- `{output_key}_index_arn` — ARN of created index.
- `{output_key}_distance_metric` — Configured distance metric.
- `{output_key}_data_type` — Configured data type.
- `{output_key}_dimension` — Configured vector dimension.
- `{output_key}_success` — `true` on success.

## Example

```lua
local flow = Flow.new("s3vector_create_index_example")

flow:step("create_index", nodes.s3vector_create_index({
    vector_bucket_name = "ironflow-vectors-demo",
    index_name = "ironflow-demo-index",
    data_type = "float32",
    distance_metric = "cosine",
    dimension = 3,
    output_key = "index"
}))

return flow
```
