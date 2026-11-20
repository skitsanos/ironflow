# `s3vector_delete_index`

Permanently delete an Amazon S3 Vectors index and every vector in it.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `vector_bucket_name` | string | With `index_name`* | — | Bucket that owns the named index |
| `bucket` | string | No | — | Alias for `vector_bucket_name`; both must resolve to the same value |
| `index_name` | string | One of* | — | Index name |
| `index` | string | No | — | Alias for `index_name`; both must resolve to the same value |
| `index_arn` | string | One of* | — | Index ARN, used alone instead of bucket and index names |
| `region` | string | No | AWS region chain | Region; when absent, `S3VECTORS_REGION`, `S3_REGION`, `AWS_REGION`, then `AWS_DEFAULT_REGION` |
| `endpoint_url` | string | No | `AWS_ENDPOINT_URL` | Service endpoint override |
| `output_key` | string | No | `"s3vector"` | Prefix for output keys |

\* Give a bucket name plus an index name, or `index_arn` alone, in the step.
A bucket ARN is not accepted. Bucket and index environment variables are never
used for deletion.

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}_bucket_name` | string | Owning bucket name, when the step used names |
| `{output_key}_index_name` | string | Deleted index name, when the step used names |
| `{output_key}_index_arn` | string | Deleted index ARN, when the step used an ARN |
| `{output_key}_success` | boolean | `true` after the service accepts the deletion |

## Behavior

- The service deletes a non-empty index; delete known vectors first only if the flow needs per-key output.
- A missing, incomplete, or conflicting target, or one that is not a string or is blank after interpolation, fails the step before any request.
- A `region` or `endpoint_url` that is not a string or is blank fails the step; the environment is used only when the parameter is absent.
- Credentials come from the AWS SDK credential chain and need `s3vectors:DeleteIndex`.
- With `IRONFLOW_BLOCK_PRIVATE_NETWORK=true`, a private endpoint needs an operator allowlist entry; see [Outbound destinations](../SECURITY.md#outbound-destinations).

## Examples

The runnable [`s3vector_vector_workflow.lua`](../../examples/16-s3vector/s3vector_vector_workflow.lua)
shows the full vectors, index, bucket teardown order.

### Delete after the vectors

```lua
flow:step("delete_index", nodes.s3vector_delete_index({
    vector_bucket_name = "${ctx.bucket_bucket_name}",
    index_name = "${ctx.index_index_name}",
    output_key = "deleted_index"
})):depends_on("delete_vectors")
```
