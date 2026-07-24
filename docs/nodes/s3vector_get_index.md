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
| `vector_bucket_name` | string | yes* | `S3VECTOR_BUCKET_NAME` / `S3_BUCKET` env vars | Bucket that owns the index. |
| `bucket` | string | no | no | Alias for `vector_bucket_name`. |
| `index_name` | string | yes* | `S3VECTOR_INDEX_NAME` env var | Target index name. |
| `index` | string | no | no | Alias for `index_name`. |
| `index_arn` | string | no | `S3VECTOR_INDEX_ARN` env var | Alternative to `index_name`. |
| `region` | string | no | AWS region chain | Override `S3VECTORS_REGION`, `S3_REGION`, `AWS_REGION`, or `AWS_DEFAULT_REGION`. |
| `endpoint_url` | string | no | `AWS_ENDPOINT_URL` env var | Override the S3 Vectors service endpoint. |
| `output_key` | string | no | `s3vector` | Prefix for context output keys. |

`index_name`/`index` require a bucket name unless `index_arn` is provided. The
S3 Vectors API does not accept a bucket ARN together with an index name.

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
