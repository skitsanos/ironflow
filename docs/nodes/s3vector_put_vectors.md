# `s3vector_put_vectors`

Store vectors in an Amazon S3 Vector index.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `vector_bucket_name` | string | yes* | `S3VECTOR_BUCKET_NAME` / `S3_BUCKET` env vars | Target bucket for the index. |
| `bucket` | string | no | no | Alias for `vector_bucket_name`. |
| `vector_bucket_arn` | string | no | `S3VECTOR_BUCKET_ARN` env var | Alternative to `vector_bucket_name`. |
| `index_name` | string | yes* | `S3VECTOR_INDEX_NAME` env var | Target index name. |
| `index` | string | no | no | Alias for `index_name`. |
| `index_arn` | string | no | `S3VECTOR_INDEX_ARN` env var | Alternative to `index_name`. |
| `vectors` | array<object> | yes | -- | Vector payload list. Each item requires `key` and `data`, optional `metadata`. |
| `vectors_source_key` | string | no | -- | Context key containing the vector array (alternative to `vectors`). |
| `output_key` | string | no | `s3vector` | Prefix for context output keys. |

Each vector object in `vectors` must contain:
- `key` (string): vector key.
- `data` (array<number>): numeric vector values.
- `metadata` (object, optional): metadata map associated with the vector.

`index_name`/`index` require a bucket reference unless `index_arn` is provided.

## Context Output

- `{output_key}_vector_count` — Number of vectors attempted to store.
- `{output_key}_vector_keys` — List of vector keys sent.
- `{output_key}_success` — `true` on success.

## Example

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
