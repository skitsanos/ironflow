# `s3vector_get_index`

Get metadata for an Amazon S3 Vector index.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `vector_bucket_name` | string | yes* | `S3VECTOR_BUCKET_NAME` / `S3_BUCKET` env vars | Bucket that owns the index. |
| `bucket` | string | no | no | Alias for `vector_bucket_name`. |
| `vector_bucket_arn` | string | no | `S3VECTOR_BUCKET_ARN` env var | Alternative to `vector_bucket_name`. |
| `index_name` | string | yes* | `S3VECTOR_INDEX_NAME` env var | Target index name. |
| `index` | string | no | no | Alias for `index_name`. |
| `index_arn` | string | no | `S3VECTOR_INDEX_ARN` env var | Alternative to `index_name`. |
| `output_key` | string | no | `s3vector` | Prefix for context output keys. |

`index_name`/`index` require a bucket reference unless `index_arn` is provided.

## Context Output

- `{output_key}_index_name` — Returned index name.
- `{output_key}_index_arn` — Returned index ARN.
- `{output_key}_bucket_name` — Owning bucket name.
- `{output_key}_dimension` — Vector dimension configured for this index.
- `{output_key}_distance_metric` — Index distance metric.
- `{output_key}_data_type` — Index data type.
- `{output_key}_creation_time` — ISO timestamp string.
- `{output_key}_metadata_configuration` — Metadata configuration (best-effort string encoding).
- `{output_key}_success` — `true` on success.

## Example

```lua
local flow = Flow.new("s3vector_get_index_example")

flow:step("get_index", nodes.s3vector_get_index({
    vector_bucket_name = "ironflow-vectors-demo",
    index_name = "ironflow-demo-index",
    output_key = "index"
}))

return flow
```
