# `s3vector_put_vectors`

Store vectors in an Amazon S3 Vector index.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `vector_bucket_name` | string | yes* | environment-only fallback | Bucket that owns a named index. Uses `S3VECTOR_BUCKET_NAME`, then legacy `S3_BUCKET`, only when no target field is configured. |
| `bucket` | string | no | no | Alias for `vector_bucket_name`. |
| `index_name` | string | yes* | environment-only fallback | Target index name. Uses `S3VECTOR_INDEX_NAME` only when no target field is configured. |
| `index` | string | no | no | Alias for `index_name`. |
| `index_arn` | string | yes* | environment-only fallback | Self-contained alternative target. Uses `S3VECTOR_INDEX_ARN` only when no target field is configured. |
| `vectors` | array<object> | yes | -- | Vector payload list. Each item requires `key` and `data`, optional `metadata`. |
| `vectors_source_key` | string | no | -- | Context key containing the vector array (alternative to `vectors`). |
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

## Vector Payload

Each vector object in `vectors` must contain:
- `key` (string): vector key.
- `data` (array<number>): numeric vector values.
- `metadata` (object, optional): metadata map associated with the vector.

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
