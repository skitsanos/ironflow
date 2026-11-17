# `s3vector_query_vectors`

Find the nearest vectors to a query vector in an Amazon S3 Vectors index,
optionally scoped by a metadata filter.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `vector_bucket_name` | string | With `index_name`* | `S3VECTOR_BUCKET_NAME`, then `S3_BUCKET` | Bucket that owns the named index |
| `bucket` | string | No | — | Alias for `vector_bucket_name`; both must resolve to the same value |
| `index_name` | string | One of* | `S3VECTOR_INDEX_NAME` | Index name |
| `index` | string | No | — | Alias for `index_name`; both must resolve to the same value |
| `index_arn` | string | One of* | `S3VECTOR_INDEX_ARN` | Index ARN, used alone instead of bucket and index names |
| `top_k` | integer | Yes | — | Number of nearest neighbors, 1 to 2147483647; accepts an interpolated integer string |
| `query_vector` | array of numbers | One of | — | Non-empty query embedding of finite numbers |
| `query_vector_key` | string | One of | — | Context key holding the query embedding array |
| `filter` | object | No | — | Metadata filter written in the flow; string values are interpolated |
| `filter_key` | string | No | — | Context key holding the filter object |
| `return_metadata` | boolean | No | `false` | Include each result's metadata |
| `return_distance` | boolean | No | `false` | Include each result's distance |
| `min_similarity` | number | No | — | Drop results whose cosine similarity (`1 - distance`) is below this value, 0 to 1 |
| `strict` | boolean | No | `false` | With `min_similarity`, fail on a non-cosine index instead of ignoring the threshold |
| `region` | string | No | AWS region chain | Region; when absent, `S3VECTORS_REGION`, `S3_REGION`, `AWS_REGION`, then `AWS_DEFAULT_REGION` |
| `endpoint_url` | string | No | `AWS_ENDPOINT_URL` | Service endpoint override |
| `output_key` | string | No | `"s3vector"` | Prefix for output keys |

\* Give a bucket name plus an index name, or `index_arn` alone. A bucket ARN
is not accepted. If any target parameter is set, the whole target must come
from the step; the environment variables are used only when none is set.

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}_vectors` | array | Result objects with `key`, plus `distance` and `metadata` when requested |
| `{output_key}_count` | number | Number of results after `min_similarity` filtering |
| `{output_key}_distance_metric` | string | Index distance metric, when the service returns it |
| `{output_key}_min_similarity` | number | Threshold, when `min_similarity` is set |
| `{output_key}_min_similarity_applied` | boolean | Whether the threshold was applied, when `min_similarity` is set |
| `{output_key}_success` | boolean | `true` on success |

## Behavior

- `query_vector` wins when both it and `query_vector_key` are set; a missing context key fails the step.
- `min_similarity` applies only to a cosine index; on another metric it is ignored (`_min_similarity_applied = false`) unless `strict = true`, which fails the step. Fewer than `top_k` results may remain.
- A target, `top_k`, query vector, flag, or filter that fails validation fails the step before any request.
- A `region` or `endpoint_url` that is not a string or is blank fails the step; the environment is used only when the parameter is absent.
- Credentials come from the AWS SDK credential chain.
- With `IRONFLOW_BLOCK_PRIVATE_NETWORK=true`, a private endpoint needs an operator allowlist entry; see [Outbound destinations](../SECURITY.md#outbound-destinations).

## Boolean Flags

`return_metadata`, `return_distance`, and `strict` default to `false` only when
absent. A set value must be a boolean, one of the strings `true`/`yes`/`on`/`1`
or `false`/`no`/`off`/`0` (any case), or a `${ctx.key}` template resolving to
one. Anything else, including a number or a misspelling, fails the step.

## Metadata Filter

Without `filter` and `filter_key` the query covers the whole index. When both
are set, `filter` is used and `filter_key` is not read.

`filter` (inline):

- Every string value, at any depth, is interpolated once; object keys are sent as written. `\${ctx.key}` keeps a literal `${ctx.key}`.
- A placeholder must resolve to a string, number, or boolean. A missing, `null`, object, or array value fails the step instead of becoming an empty string.
- Interpolation produces text: `"${ctx.year}"` sends `"2026"`. For typed values or a whole sub-filter, build the filter in a step and use `filter_key`.

`filter_key` (from context):

- The key must exist and hold a non-empty object; a missing key, `null`, `{}`, an array, or a scalar fails the step.
- The stored object is sent as is; placeholders inside it are not expanded.

```lua
filter = {
    tenant = "${ctx.tenant_id}",
    ["$and"] = { { kind = "note-${ctx.kind}" } }
}
```

## Examples

### Query with distances and metadata

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

### Filter built in an earlier step

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
