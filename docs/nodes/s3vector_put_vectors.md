# `s3vector_put_vectors`

Store vectors, with optional metadata, in an Amazon S3 Vectors index.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `vector_bucket_name` | string | With `index_name`* | `S3VECTOR_BUCKET_NAME`, then `S3_BUCKET` | Bucket that owns the named index |
| `bucket` | string | No | — | Alias for `vector_bucket_name`; both must resolve to the same value |
| `index_name` | string | One of* | `S3VECTOR_INDEX_NAME` | Index name |
| `index` | string | No | — | Alias for `index_name`; both must resolve to the same value |
| `index_arn` | string | One of* | `S3VECTOR_INDEX_ARN` | Index ARN, used alone instead of bucket and index names |
| `vectors` | array | One of | — | Vector objects (see below); each `key` supports `${ctx.key}` |
| `vectors_source_key` | string | One of | — | Context key holding the vector array; its keys are used as stored, not interpolated |
| `region` | string | No | AWS region chain | Region; when absent, `S3VECTORS_REGION`, `S3_REGION`, `AWS_REGION`, then `AWS_DEFAULT_REGION` |
| `endpoint_url` | string | No | `AWS_ENDPOINT_URL` | Service endpoint override |
| `output_key` | string | No | `"s3vector"` | Prefix for output keys |

\* Give a bucket name plus an index name, or `index_arn` alone. A bucket ARN
is not accepted. If any target parameter is set, the whole target must come
from the step; the environment variables are used only when none is set.

Each vector object has:

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `key` | string | Yes | Vector key |
| `data` | array of numbers | Yes | Non-empty list of finite numbers, sent as `float32` |
| `metadata` | any JSON value | No | Metadata stored with the vector, usually an object |

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}_vector_count` | number | Number of vectors sent |
| `{output_key}_vector_keys` | array | Keys sent, in order |
| `{output_key}_success` | boolean | `true` on success |

## Behavior

- `vectors` wins when both `vectors` and `vectors_source_key` are set.
- An empty vector list, a missing `key` or `data`, empty `data`, or a non-numeric or non-finite value fails the step before any request.
- An incomplete or conflicting target, or one that is not a string or is blank after interpolation, fails the step.
- A `region` or `endpoint_url` that is not a string or is blank fails the step; the environment is used only when the parameter is absent.
- Credentials come from the AWS SDK credential chain.
- With `IRONFLOW_BLOCK_PRIVATE_NETWORK=true`, a private endpoint needs an operator allowlist entry; see [Outbound destinations](../SECURITY.md#outbound-destinations).

## Examples

### Store two vectors

```lua
local flow = Flow.new("s3vector_put_vectors_example")

flow:step("put_vectors", nodes.s3vector_put_vectors({
    vector_bucket_name = "ironflow-vectors-demo",
    index_name = "ironflow-demo-index",
    vectors = {
        { key = "doc-1", data = { 0.11, 0.22, 0.33 }, metadata = { source = "docs" } },
        { key = "doc-2", data = { 0.41, 0.52, 0.63 }, metadata = { source = "docs" } },
    },
    output_key = "put"
}))

return flow
```
