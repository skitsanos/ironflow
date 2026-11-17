# `s3vector_create_bucket`

Create an Amazon S3 Vectors bucket.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `vector_bucket_name` | string | Yes* | `S3VECTOR_BUCKET_NAME`, then `S3_BUCKET` | Name of the bucket to create |
| `bucket` | string | No | — | Alias for `vector_bucket_name`; both must resolve to the same value |
| `region` | string | No | AWS region chain | Region; when absent, `S3VECTORS_REGION`, `S3_REGION`, `AWS_REGION`, then `AWS_DEFAULT_REGION` |
| `endpoint_url` | string | No | `AWS_ENDPOINT_URL` | Service endpoint override |
| `output_key` | string | No | `"s3vector"` | Prefix for output keys |

\* The environment variables are used only when neither `vector_bucket_name`
nor `bucket` is set.

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}_bucket_name` | string | Bucket name sent in the request |
| `{output_key}_bucket_arn` | string | Bucket ARN, when the service returns one |
| `{output_key}_success` | boolean | `true` on success |

## Behavior

- `vector_bucket_arn` is not accepted; a bucket is created by name only.
- A target, `region`, or `endpoint_url` that is not a string or is blank after interpolation fails the step before any request.
- Credentials come from the AWS SDK credential chain.
- With `IRONFLOW_BLOCK_PRIVATE_NETWORK=true`, a private endpoint needs an operator allowlist entry; see [Outbound destinations](../SECURITY.md#outbound-destinations).

## Examples

### Create a bucket

```lua
local flow = Flow.new("s3vector_create_bucket_example")

flow:step("create_bucket", nodes.s3vector_create_bucket({
    vector_bucket_name = "ironflow-vectors-demo",
    output_key = "bucket"
}))

return flow
```
