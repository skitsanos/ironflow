# `s3vector_create_bucket`

Create a new Amazon S3 Vector bucket.

## Parameters

| Parameter | Type   | Required | Default | Description |
|-----------|--------|----------|---------|-------------|
| `vector_bucket_name` | string | yes | `S3VECTOR_BUCKET_NAME` / `S3_BUCKET` env vars | Bucket name. |
| `bucket` | string | no | no | Alias for `vector_bucket_name`. |
| `region` | string | no | AWS region chain | Override `S3VECTORS_REGION`, `S3_REGION`, `AWS_REGION`, or `AWS_DEFAULT_REGION`. |
| `endpoint_url` | string | no | `AWS_ENDPOINT_URL` env var | Override the S3 Vectors service endpoint. |
| `output_key` | string | no | `s3vector` | Prefix for context output keys. |

## Context Output

- `{output_key}_bucket_name` — Bucket name used/created.
- `{output_key}_bucket_arn` — Bucket ARN if returned by the service.
- `{output_key}_success` — `true` on success.

## Example

```lua
local flow = Flow.new("s3vector_create_bucket_example")

flow:step("create_bucket", nodes.s3vector_create_bucket({
    vector_bucket_name = "ironflow-vectors-demo",
    output_key = "bucket"
}))

return flow
```
