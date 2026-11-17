# `s3vector_delete_vectors`

Delete vectors by key from an Amazon S3 Vectors index.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `vector_bucket_name` | string | With `index_name`* | — | Bucket that owns the named index |
| `bucket` | string | No | — | Alias for `vector_bucket_name`; both must resolve to the same value |
| `index_name` | string | One of* | — | Index name |
| `index` | string | No | — | Alias for `index_name`; both must resolve to the same value |
| `index_arn` | string | One of* | — | Index ARN, used alone instead of bucket and index names |
| `keys` | array of strings | One of | — | Keys to delete; each item supports `${ctx.key}` |
| `keys_source_key` | string | One of | — | Context key holding a string array of keys; items are used as stored |
| `region` | string | No | AWS region chain | Region; when absent, `S3VECTORS_REGION`, `S3_REGION`, `AWS_REGION`, then `AWS_DEFAULT_REGION` |
| `endpoint_url` | string | No | `AWS_ENDPOINT_URL` | Service endpoint override |
| `output_key` | string | No | `"s3vector"` | Prefix for output keys |

\* Give a bucket name plus an index name, or `index_arn` alone, in the step.
A bucket ARN is not accepted. Bucket and index environment variables are never
used for deletion.

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}_deleted_count` | number | Number of keys submitted for deletion |
| `{output_key}_deleted_keys` | array | Keys submitted, in order |
| `{output_key}_success` | boolean | `true` on success |

## Behavior

- `keys` wins when both `keys` and `keys_source_key` are set.
- An empty key list, a non-string key, or a missing source key fails the step before any request.
- A missing, incomplete, or conflicting target, or one that is not a string or is blank after interpolation, fails the step.
- A `region` or `endpoint_url` that is not a string or is blank fails the step; the environment is used only when the parameter is absent.
- Credentials come from the AWS SDK credential chain.
- With `IRONFLOW_BLOCK_PRIVATE_NETWORK=true`, a private endpoint needs an operator allowlist entry; see [Outbound destinations](../SECURITY.md#outbound-destinations).

For full teardown, run this node, then [`s3vector_delete_index`](s3vector_delete_index.md),
then [`s3vector_delete_bucket`](s3vector_delete_bucket.md).

## Examples

### Delete two keys

```lua
local flow = Flow.new("s3vector_delete_vectors_example")

flow:step("delete_vectors", nodes.s3vector_delete_vectors({
    vector_bucket_name = "ironflow-vectors-demo",
    index_name = "ironflow-demo-index",
    keys = { "doc-1", "doc-2" },
    output_key = "delete"
}))

return flow
```
