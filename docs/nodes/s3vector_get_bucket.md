# `s3vector_get_bucket`

Get metadata for an existing Amazon S3 Vector bucket.

## Parameters

| Parameter | Type   | Required | Default | Description |
|-----------|--------|----------|---------|-------------|
| `vector_bucket_name` | string | yes* | `S3VECTOR_BUCKET_NAME` env var | Bucket name to query. |
| `bucket` | string | no | no | Alias for `vector_bucket_name`. |
| `vector_bucket_arn` | string | yes* | `S3VECTOR_BUCKET_ARN` env var | Bucket ARN to query. |
| `output_key` | string | no | `s3vector` | Prefix for context output keys. |

`vector_bucket_name` and `vector_bucket_arn` are alternatives; provide one of them.

## Context Output

- `{output_key}_bucket_name` — Returned bucket name.
- `{output_key}_bucket_arn` — Returned bucket ARN.
- `{output_key}_creation_time` — ISO timestamp string.
- `{output_key}_encryption_configuration` — Encryption configuration object (best-effort string encoding).
- `{output_key}_success` — `true` on success.

## Example

```lua
local flow = Flow.new("s3vector_get_bucket_example")

flow:step("get_bucket", nodes.s3vector_get_bucket({
    vector_bucket_name = "ironflow-vectors-demo",
    output_key = "bucket"
}))

return flow
```
