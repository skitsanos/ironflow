# `s3vector_delete_vectors`

Delete vectors by keys from an Amazon S3 Vector index.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `vector_bucket_name` | string | yes* | `S3VECTOR_BUCKET_NAME` / `S3_BUCKET` env vars | Bucket that owns the index. |
| `bucket` | string | no | no | Alias for `vector_bucket_name`. |
| `vector_bucket_arn` | string | no | `S3VECTOR_BUCKET_ARN` env var | Alternative to `vector_bucket_name`. |
| `index_name` | string | yes* | `S3VECTOR_INDEX_NAME` env var | Target index name. |
| `index` | string | no | no | Alias for `index_name`. |
| `index_arn` | string | no | `S3VECTOR_INDEX_ARN` env var | Alternative to `index_name`. |
| `keys` | array<string> | yes | -- | Keys to delete from the index. |
| `keys_source_key` | string | no | -- | Context key containing string-array keys. |
| `output_key` | string | no | `s3vector` | Prefix for context output keys. |

`index_name`/`index` require a bucket reference unless `index_arn` is provided.

## Context Output

- `{output_key}_deleted_count` — Number of keys submitted for deletion.
- `{output_key}_deleted_keys` — Key list.
- `{output_key}_success` — `true` on success.

## Example

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
