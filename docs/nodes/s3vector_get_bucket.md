# `s3vector_get_bucket`

Get metadata for an existing Amazon S3 Vector bucket.

This node is read-only but requires S3 Vectors service access through the AWS
SDK credential chain and configured region/endpoint. The runnable
[`s3vector_vector_workflow.lua`](../../examples/16-s3vector/s3vector_vector_workflow.lua)
creates the bucket and index it inspects. It deletes demo vectors on the success
path, then deletes the index and bucket. An earlier failure or interrupted run
can still skip that success-dependent cleanup.

## Parameters

| Parameter | Type   | Required | Default | Description |
|-----------|--------|----------|---------|-------------|
| `vector_bucket_name` | string | yes* | `S3VECTOR_BUCKET_NAME` / `S3_BUCKET` env vars | Bucket name to query. |
| `bucket` | string | no | no | Alias for `vector_bucket_name`. |
| `vector_bucket_arn` | string | yes* | `S3VECTOR_BUCKET_ARN` env var | Bucket ARN to query. |
| `region` | string | no | AWS region chain | Override `S3VECTORS_REGION`, `S3_REGION`, `AWS_REGION`, or `AWS_DEFAULT_REGION`. |
| `endpoint_url` | string | no | `AWS_ENDPOINT_URL` env var | Override the S3 Vectors service endpoint. |
| `output_key` | string | no | `s3vector` | Prefix for context output keys. |

`vector_bucket_name` and `vector_bucket_arn` are alternatives; provide one of them.

## Context Output

- `{output_key}_bucket_name` — Returned bucket name.
- `{output_key}_bucket_arn` — Returned bucket ARN.
- `{output_key}_creation_time` — ISO timestamp string.
- `{output_key}_encryption_configuration` — Encryption configuration object (best-effort string encoding).
- `{output_key}_success` — `true` on success.

## Example

See the linked workflow for the executable, cataloged example. Minimal use:

```lua
local flow = Flow.new("s3vector_get_bucket_example")

flow:step("get_bucket", nodes.s3vector_get_bucket({
    vector_bucket_name = "ironflow-vectors-demo",
    output_key = "bucket"
}))

return flow
```
