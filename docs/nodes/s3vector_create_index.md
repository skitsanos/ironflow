# `s3vector_create_index`

Create an Amazon S3 Vector index inside a bucket.

## Parameters

| Parameter | Type   | Required | Default | Description |
|-----------|--------|----------|---------|-------------|
| `vector_bucket_name` | string | yes* | environment-only fallback | Target bucket name. Uses `S3VECTOR_BUCKET_NAME`, then legacy `S3_BUCKET`, only when no target field is configured. |
| `bucket` | string | no | no | Alias for `vector_bucket_name`. |
| `vector_bucket_arn` | string | yes* | environment-only fallback | Alternative target bucket ARN. Uses `S3VECTOR_BUCKET_ARN` only when no target field is configured. |
| `index_name` | string | yes | environment-only fallback | Name for the new index. Uses `S3VECTOR_INDEX_NAME` only when no target field is configured. |
| `index` | string | no | no | Alias for `index_name`. |
| `data_type` | string | yes | -- | Vector type (for example: `float32`). |
| `distance_metric` | string | yes | -- | Similarity metric (for example: `euclidean`, `cosine`). |
| `dimension` | integer | yes | -- | Vector dimension (must be > 0). |
| `region` | string | no | AWS region chain | Override `S3VECTORS_REGION`, `S3_REGION`, `AWS_REGION`, or `AWS_DEFAULT_REGION`. |
| `endpoint_url` | string | no | `AWS_ENDPOINT_URL` env var | Override the S3 Vectors service endpoint. |
| `output_key` | string | no | `s3vector` | Prefix for context output keys. |

## Target Resolution

The supported target shapes are a bucket name plus an index name, or a bucket
ARN plus an index name. If any bucket or index target field is configured, all
fields needed by that shape must come from node configuration after
interpolation; identifier environment variables neither complete nor override
it. With no configured target field, the node accepts the same coherent
environment-only shapes from `S3VECTOR_INDEX_NAME` plus either
`S3VECTOR_BUCKET_NAME` (falling back to legacy `S3_BUCKET`) or
`S3VECTOR_BUCKET_ARN`. Conflicting forms and non-string, incomplete, or blank
targets fail before the AWS client is built.

## Context Output

- `{output_key}_index_name` — Created index name.
- `{output_key}_bucket_name` — Bucket name used for the name target form.
- `{output_key}_bucket_arn` — Bucket ARN used for the ARN target form.
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
