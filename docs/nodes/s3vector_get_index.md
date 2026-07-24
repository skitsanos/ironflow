# `s3vector_get_index`

Get metadata for an Amazon S3 Vector index.

This node is read-only but requires S3 Vectors service access through the AWS
SDK credential chain and configured region/endpoint. The runnable
[`s3vector_vector_workflow.lua`](../../examples/16-s3vector/s3vector_vector_workflow.lua)
creates the bucket and index it inspects. It deletes demo vectors on the success
path, then deletes the index and bucket. An earlier failure or interrupted run
can still skip that success-dependent cleanup.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `vector_bucket_name` | string | yes* | environment-only fallback | Bucket that owns a named index. Uses `S3VECTOR_BUCKET_NAME`, then legacy `S3_BUCKET`, only when no target field is configured. |
| `bucket` | string | no | no | Alias for `vector_bucket_name`. |
| `index_name` | string | yes* | environment-only fallback | Target index name. Uses `S3VECTOR_INDEX_NAME` only when no target field is configured. |
| `index` | string | no | no | Alias for `index_name`. |
| `index_arn` | string | yes* | environment-only fallback | Self-contained alternative target. Uses `S3VECTOR_INDEX_ARN` only when no target field is configured. |
| `region` | string | no | AWS region chain | Override `S3VECTORS_REGION`, `S3_REGION`, `AWS_REGION`, or `AWS_DEFAULT_REGION`. |
| `endpoint_url` | string | no | `AWS_ENDPOINT_URL` env var | Override the S3 Vectors service endpoint. |
| `output_key` | string | no | `s3vector` | Prefix for context output keys. |

## Target Resolution

Use exactly one supported shape: `vector_bucket_name`/`bucket` plus
`index_name`/`index`, or `index_arn` by itself. Bucket ARN plus index name is not
supported. If any target field is configured, the complete shape must come from
node configuration after interpolation; identifier environment variables
neither complete nor override it. With no configured target field, the node
accepts the same coherent environment-only shapes from
`S3VECTOR_BUCKET_NAME` (falling back to legacy `S3_BUCKET`) plus
`S3VECTOR_INDEX_NAME`, or from `S3VECTOR_INDEX_ARN` alone. Conflicting forms and
non-string, incomplete, or blank targets fail before the AWS client is built.

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

See the linked workflow for the executable, cataloged example. Minimal use:

```lua
local flow = Flow.new("s3vector_get_index_example")

flow:step("get_index", nodes.s3vector_get_index({
    vector_bucket_name = "ironflow-vectors-demo",
    index_name = "ironflow-demo-index",
    output_key = "index"
}))

return flow
```
