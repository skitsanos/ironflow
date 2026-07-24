# `s3vector_delete_bucket`

Delete an Amazon S3 Vector bucket.

This operation requires the `s3vectors:DeleteVectorBucket` IAM permission. The
service rejects deletion while the bucket contains an index or has an operation
in progress, so delete every index first.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `vector_bucket_name` | string | yes* | -- | Explicit bucket name to delete. |
| `bucket` | string | no | no | Alias for `vector_bucket_name`. |
| `vector_bucket_arn` | string | yes* | -- | Explicit alternative bucket ARN. |
| `region` | string | no | AWS region chain | Override `S3VECTORS_REGION`, `S3_REGION`, `AWS_REGION`, or `AWS_DEFAULT_REGION`. |
| `endpoint_url` | string | no | `AWS_ENDPOINT_URL` env var | Override the S3 Vectors service endpoint. |
| `output_key` | string | no | `s3vector` | Prefix for context output keys. |

Provide either `vector_bucket_name`/`bucket` or `vector_bucket_arn`, but not
both. Resource identifiers are deliberately not read from environment
variables for this destructive operation.

## Context Output

- `{output_key}_bucket_name` — Deleted bucket name when deletion used a name.
- `{output_key}_bucket_arn` — Deleted bucket ARN when deletion used an ARN.
- `{output_key}_success` — `true` after the service accepts the deletion.

The service returns no resource body; the identifier field above echoes the
validated request target.

## Example

```lua
flow:step("delete_bucket", nodes.s3vector_delete_bucket({
    vector_bucket_name = "${ctx.bucket_bucket_name}",
    output_key = "deleted_bucket"
})):depends_on("delete_index")
```

See the runnable
[`s3vector_vector_workflow.lua`](../../examples/16-s3vector/s3vector_vector_workflow.lua)
for the complete vectors → index → bucket teardown order.
