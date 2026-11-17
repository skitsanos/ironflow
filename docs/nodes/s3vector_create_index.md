# `s3vector_create_index`

Create a vector index inside an Amazon S3 Vectors bucket.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `vector_bucket_name` | string | One of* | `S3VECTOR_BUCKET_NAME`, then `S3_BUCKET` | Bucket name |
| `bucket` | string | No | — | Alias for `vector_bucket_name`; both must resolve to the same value |
| `vector_bucket_arn` | string | One of* | `S3VECTOR_BUCKET_ARN` | Bucket ARN, instead of a name |
| `index_name` | string | Yes* | `S3VECTOR_INDEX_NAME` | Name of the new index |
| `index` | string | No | — | Alias for `index_name`; both must resolve to the same value |
| `data_type` | string | Yes | — | Vector element type; `float32` is the only value |
| `distance_metric` | string | Yes | — | `cosine` or `euclidean` |
| `dimension` | integer | Yes | — | Vector dimension, 1 to 2147483647; accepts an interpolated integer string |
| `region` | string | No | AWS region chain | Region; when absent, `S3VECTORS_REGION`, `S3_REGION`, `AWS_REGION`, then `AWS_DEFAULT_REGION` |
| `endpoint_url` | string | No | `AWS_ENDPOINT_URL` | Service endpoint override |
| `output_key` | string | No | `"s3vector"` | Prefix for output keys |

\* Give an index name plus exactly one of bucket name or bucket ARN. If any
bucket or index parameter is set, the whole target must come from the step;
the environment variables are used only when none is set.

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}_index_name` | string | Created index name |
| `{output_key}_bucket_name` | string | Bucket name, when the step used a name |
| `{output_key}_bucket_arn` | string | Bucket ARN, when the step used an ARN |
| `{output_key}_index_arn` | string | Index ARN, when the service returns one |
| `{output_key}_data_type` | string | Data type sent |
| `{output_key}_distance_metric` | string | Distance metric sent |
| `{output_key}_dimension` | number | Dimension sent |
| `{output_key}_success` | boolean | `true` on success |

## Behavior

- `index_arn` is not accepted; an index is created by name.
- An unknown `data_type` or `distance_metric`, a dimension of `0` or above 2147483647, or an incomplete or conflicting target fails the step before any request.
- A `region` or `endpoint_url` that is not a string or is blank fails the step; the environment is used only when the parameter is absent.
- Credentials come from the AWS SDK credential chain.
- With `IRONFLOW_BLOCK_PRIVATE_NETWORK=true`, a private endpoint needs an operator allowlist entry; see [Outbound destinations](../SECURITY.md#outbound-destinations).

## Examples

### Create a cosine index

```lua
local flow = Flow.new("s3vector_create_index_example")

flow:step("create_index", nodes.s3vector_create_index({
    vector_bucket_name = "ironflow-vectors-demo",
    index_name = "ironflow-demo-index",
    data_type = "float32",
    distance_metric = "cosine",
    dimension = 3,
    output_key = "index"
}))

return flow
```
