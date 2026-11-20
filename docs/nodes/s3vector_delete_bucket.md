# `s3vector_delete_bucket`

Delete an Amazon S3 Vectors bucket. The bucket must have no indexes.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `vector_bucket_name` | string | One of* | — | Bucket name |
| `bucket` | string | No | — | Alias for `vector_bucket_name`; both must resolve to the same value |
| `vector_bucket_arn` | string | One of* | — | Bucket ARN, instead of a name |
| `region` | string | No | AWS region chain | Region; when absent, `S3VECTORS_REGION`, `S3_REGION`, `AWS_REGION`, then `AWS_DEFAULT_REGION` |
| `endpoint_url` | string | No | `AWS_ENDPOINT_URL` | Service endpoint override |
| `output_key` | string | No | `"s3vector"` | Prefix for output keys |

\* Set exactly one of name or ARN in the step. Bucket environment variables
are never used for deletion.

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}_bucket_name` | string | Deleted bucket name, when the step used a name |
| `{output_key}_bucket_arn` | string | Deleted bucket ARN, when the step used an ARN |
| `{output_key}_success` | boolean | `true` after the service accepts the deletion |

## Behavior

- The service rejects deletion while the bucket still has an index; delete vectors, then indexes, then the bucket.
- A missing target, both a name and an ARN, or a target that is not a string or is blank after interpolation fails the step before any request.
- A `region` or `endpoint_url` that is not a string or is blank fails the step; the environment is used only when the parameter is absent.
- Credentials come from the AWS SDK credential chain and need `s3vectors:DeleteVectorBucket`.
- With `IRONFLOW_BLOCK_PRIVATE_NETWORK=true`, a private endpoint needs an operator allowlist entry; see [Outbound destinations](../SECURITY.md#outbound-destinations).

## Examples

The runnable [`s3vector_vector_workflow.lua`](../../examples/16-s3vector/s3vector_vector_workflow.lua)
shows the full vectors, index, bucket teardown order.

### Delete after the index

```lua
flow:step("delete_bucket", nodes.s3vector_delete_bucket({
    vector_bucket_name = "${ctx.bucket_bucket_name}",
    output_key = "deleted_bucket"
})):depends_on("delete_index")
```
