# `s3vector_query_vectors`

Run vector similarity search on an Amazon S3 Vector index.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `vector_bucket_name` | string | yes* | environment-only fallback | Bucket that owns a named index. Uses `S3VECTOR_BUCKET_NAME`, then legacy `S3_BUCKET`, only when no target field is configured. |
| `bucket` | string | no | no | Alias for `vector_bucket_name`. |
| `index_name` | string | yes* | environment-only fallback | Target index name. Uses `S3VECTOR_INDEX_NAME` only when no target field is configured. |
| `index` | string | no | no | Alias for `index_name`. |
| `index_arn` | string | yes* | environment-only fallback | Self-contained alternative target. Uses `S3VECTOR_INDEX_ARN` only when no target field is configured. |
| `top_k` | integer | yes | -- | Number of nearest neighbors to return (`> 0`). |
| `query_vector` | array<number> | no* | -- | Query embedding vector. |
| `query_vector_key` | string | no* | -- | Context key containing a query embedding array. |
| `filter` | object | no | -- | Optional metadata filter, written in the flow. |
| `filter_key` | string | no | -- | Context key that holds the metadata filter object. When set, the key must exist and hold a non-empty object; see [Metadata Filter](#metadata-filter). |
| `return_metadata` | bool | no | `false` | Include vector metadata in results. |
| `return_distance` | bool | no | `false` | Include distance values in results. |
| `min_similarity` | number | no | -- | Optional minimum cosine similarity threshold between query vector and results. Only supported for cosine-index metrics (`min_similarity = 1 - distance`). Fewer results may be returned than `top_k`. |
| `strict` | bool | no | `false` | When `true`, require a cosine index for `min_similarity`; otherwise `min_similarity` is ignored for non-cosine indexes. |
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

## Query Vector

At least one of `query_vector` or `query_vector_key` is required.

## Metadata Filter

Without `filter` and `filter_key` the query covers the whole index.

`filter` is sent as written in the flow. `filter_key` reads the filter from
context, and a configured `filter_key` is a requirement, not a hint: the step
fails before the AWS client is built, and no request is sent, unless the key
resolves to a filter.

| `filter_key` case | Result |
|-------------------|--------|
| Names a context key holding a non-empty object | The object is sent as the request filter. |
| Names a key that is absent from context | Fails: `requires filter_key '<key>' to exist in context`. |
| The context value is `null`, an empty object `{}`, an array, a string, a number or a boolean | Fails: `requires filter_key '<key>' to hold a non-empty filter object`. |
| Not a string, or empty after `${ctx.key}` interpolation | Fails: `requires 'filter_key' to be a non-empty string`. |

An empty object is refused because it restricts nothing: a flow that builds a
tenant or access filter upstream and ends up with `{}` would otherwise query
every vector. Error messages name the context key and never include its
value. The context value is used as stored; `${ctx.key}` placeholders inside
it are not expanded.

When both are configured, `filter` is used and `filter_key` is not read. Leave
`filter_key` out of the step (in Lua, `nil`) for a query that is meant to be
unfiltered.

```lua
flow:step("scope", nodes.code({
    source = function(ctx)
        return { tenant_filter = { tenant = ctx.tenant_id } }
    end
}))

flow:step("query_vectors", nodes.s3vector_query_vectors({
    vector_bucket_name = "ironflow-vectors-demo",
    index_name = "ironflow-demo-index",
    top_k = 5,
    query_vector_key = "query_embedding",
    filter_key = "tenant_filter",
    output_key = "query"
})):depends_on("scope")
```

## Context Output

- `{output_key}_distance_metric` — Similarity metric used.
- `{output_key}_min_similarity` — Configured minimum cosine similarity threshold (when set).
- `{output_key}_min_similarity_applied` — `true` when cosine filtering was actually applied.
- `{output_key}_count` — Number of returned result entries.
- `{output_key}_vectors` — Array of result objects (`key`, optional `distance`, optional `metadata`).
- `{output_key}_success` — `true` on success.

## Example

```lua
local flow = Flow.new("s3vector_query_vectors_example")

flow:step("query_vectors", nodes.s3vector_query_vectors({
    vector_bucket_name = "ironflow-vectors-demo",
    index_name = "ironflow-demo-index",
    top_k = 2,
    query_vector = { 0.14, 0.25, 0.31 },
    return_metadata = true,
    return_distance = true,
    output_key = "query"
}))

return flow
```
