# `s3vector_delete_index`

Permanently delete an Amazon S3 Vector index and all vector data in it.

This operation requires the `s3vectors:DeleteIndex` IAM permission. Although
the service can delete a non-empty index, lifecycle workflows should explicitly
delete known vectors first when they need auditable per-vector cleanup output.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `vector_bucket_name` | string | yes* | -- | Explicit bucket name that owns a named index. |
| `bucket` | string | no | no | Alias for `vector_bucket_name`. |
| `index_name` | string | yes* | -- | Explicit index name to delete; requires a bucket name. |
| `index` | string | no | no | Alias for `index_name`. |
| `index_arn` | string | yes* | -- | Explicit alternative that identifies the index without a bucket name. |
| `region` | string | no | AWS region chain | Override `S3VECTORS_REGION`, `S3_REGION`, `AWS_REGION`, or `AWS_DEFAULT_REGION`. |
| `endpoint_url` | string | no | `AWS_ENDPOINT_URL` env var | Override the S3 Vectors service endpoint. |
| `output_key` | string | no | `s3vector` | Prefix for context output keys. |

## Target Resolution and Safety

Use exactly one explicit configured shape after interpolation:
`vector_bucket_name`/`bucket` plus `index_name`/`index`, or `index_arn` by
itself. Do not configure bucket fields with the ARN form. Resource identifier
environment variables are deliberately never consulted for index deletion;
region, endpoint, and AWS credentials may still come from the environment.
Conflicting forms and non-string, incomplete, or blank targets fail before the
AWS client is built.

## Context Output

- `{output_key}_bucket_name` — Owning bucket name when deletion used names.
- `{output_key}_index_name` — Deleted index name when deletion used names.
- `{output_key}_index_arn` — Deleted index ARN when deletion used an ARN.
- `{output_key}_success` — `true` after the service accepts the deletion.

The service returns no resource body; the identifier fields above echo the
validated request target.

## Example

```lua
flow:step("delete_index", nodes.s3vector_delete_index({
    vector_bucket_name = "${ctx.bucket_bucket_name}",
    index_name = "${ctx.index_index_name}",
    output_key = "deleted_index"
})):depends_on("delete_vectors")
```

Delete every index before attempting to delete its vector bucket. See the
runnable [`s3vector_vector_workflow.lua`](../../examples/16-s3vector/s3vector_vector_workflow.lua)
for the complete lifecycle.
