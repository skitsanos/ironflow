# `s3vector_create_bucket`

Create a new Amazon S3 Vector bucket.

## Parameters

| Parameter | Type   | Required | Default | Description |
|-----------|--------|----------|---------|-------------|
| `vector_bucket_name` | string | yes | environment-only fallback | Bucket name. When no target field is configured, uses `S3VECTOR_BUCKET_NAME`, then legacy `S3_BUCKET`. |
| `bucket` | string | no | no | Alias for `vector_bucket_name`. |
| `region` | string | no | AWS region chain | Override `S3VECTORS_REGION`, `S3_REGION`, `AWS_REGION`, or `AWS_DEFAULT_REGION`. |
| `endpoint_url` | string | no | `AWS_ENDPOINT_URL` env var | Override the S3 Vectors service endpoint. |
| `output_key` | string | no | `s3vector` | Prefix for context output keys. |

## Target Resolution

The only supported target is a bucket name supplied as
`vector_bucket_name`/`bucket`. If either field is configured, its interpolated
value is the complete target and bucket identifier environment variables are
not consulted. Otherwise, the node uses `S3VECTOR_BUCKET_NAME`, then the legacy
`S3_BUCKET` alias. A configured target that is non-string or resolves to a blank
value fails before the AWS client is built.

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
