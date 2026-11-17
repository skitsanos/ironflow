# `s3vector_get_bucket`

Read the metadata of an existing Amazon S3 Vectors bucket.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `vector_bucket_name` | string | One of* | `S3VECTOR_BUCKET_NAME`, then `S3_BUCKET` | Bucket name |
| `bucket` | string | No | — | Alias for `vector_bucket_name`; both must resolve to the same value |
| `vector_bucket_arn` | string | One of* | `S3VECTOR_BUCKET_ARN` | Bucket ARN, instead of a name |
| `region` | string | No | AWS region chain | Region; when absent, `S3VECTORS_REGION`, `S3_REGION`, `AWS_REGION`, then `AWS_DEFAULT_REGION` |
| `endpoint_url` | string | No | `AWS_ENDPOINT_URL` | Service endpoint override |
| `output_key` | string | No | `"s3vector"` | Prefix for output keys |

\* Set exactly one of name or ARN. The environment variables are used only when
no target parameter is set; they never complete or override a configured one.

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}_bucket_name` | string | Bucket name |
| `{output_key}_bucket_arn` | string | Bucket ARN |
| `{output_key}_creation_time` | string | Creation timestamp |
| `{output_key}_encryption_configuration` | string | Debug text of the encryption settings, when present |
| `{output_key}_success` | boolean | `true` on success |

## Behavior

- Both a name and an ARN, or a target that is not a string or is blank after interpolation, fails the step before any request.
- A `region` or `endpoint_url` that is not a string or is blank fails the step; the environment is used only when the parameter is absent.
- Credentials come from the AWS SDK credential chain.
- With `IRONFLOW_BLOCK_PRIVATE_NETWORK=true`, a private endpoint needs an operator allowlist entry; see [Outbound destinations](../SECURITY.md#outbound-destinations).

## Examples

The runnable [`s3vector_vector_workflow.lua`](../../examples/16-s3vector/s3vector_vector_workflow.lua)
creates, inspects, and deletes a bucket and index.

### Read bucket metadata

```lua
local flow = Flow.new("s3vector_get_bucket_example")

flow:step("get_bucket", nodes.s3vector_get_bucket({
    vector_bucket_name = "ironflow-vectors-demo",
    output_key = "bucket"
}))

return flow
```
