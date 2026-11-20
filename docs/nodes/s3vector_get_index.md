# `s3vector_get_index`

Read the metadata of an Amazon S3 Vectors index.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `vector_bucket_name` | string | With `index_name`* | `S3VECTOR_BUCKET_NAME`, then `S3_BUCKET` | Bucket that owns the named index |
| `bucket` | string | No | — | Alias for `vector_bucket_name`; both must resolve to the same value |
| `index_name` | string | One of* | `S3VECTOR_INDEX_NAME` | Index name |
| `index` | string | No | — | Alias for `index_name`; both must resolve to the same value |
| `index_arn` | string | One of* | `S3VECTOR_INDEX_ARN` | Index ARN, used alone instead of bucket and index names |
| `region` | string | No | AWS region chain | Region; when absent, `S3VECTORS_REGION`, `S3_REGION`, `AWS_REGION`, then `AWS_DEFAULT_REGION` |
| `endpoint_url` | string | No | `AWS_ENDPOINT_URL` | Service endpoint override |
| `output_key` | string | No | `"s3vector"` | Prefix for output keys |

\* Give a bucket name plus an index name, or `index_arn` alone. A bucket ARN
is not accepted. If any target parameter is set, the whole target must come
from the step; the environment variables are used only when none is set.

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}_index_name` | string | Index name |
| `{output_key}_index_arn` | string | Index ARN |
| `{output_key}_bucket_name` | string | Owning bucket name |
| `{output_key}_dimension` | number | Vector dimension |
| `{output_key}_distance_metric` | string | Distance metric |
| `{output_key}_data_type` | string | Data type |
| `{output_key}_creation_time` | string | Creation timestamp |
| `{output_key}_metadata_configuration` | string | Debug text of the metadata settings, when present |
| `{output_key}_success` | boolean | `true` on success |

## Behavior

- An incomplete or conflicting target, or one that is not a string or is blank after interpolation, fails the step before any request.
- A `region` or `endpoint_url` that is not a string or is blank fails the step; the environment is used only when the parameter is absent.
- Credentials come from the AWS SDK credential chain.
- With `IRONFLOW_BLOCK_PRIVATE_NETWORK=true`, a private endpoint needs an operator allowlist entry; see [Outbound destinations](../SECURITY.md#outbound-destinations).

## Examples

The runnable [`s3vector_vector_workflow.lua`](../../examples/16-s3vector/s3vector_vector_workflow.lua)
creates, inspects, and deletes a bucket and index.

### Read index metadata

```lua
local flow = Flow.new("s3vector_get_index_example")

flow:step("get_index", nodes.s3vector_get_index({
    vector_bucket_name = "ironflow-vectors-demo",
    index_name = "ironflow-demo-index",
    output_key = "index"
}))

return flow
```
