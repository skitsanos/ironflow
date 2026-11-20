# `s3_list_objects`

List objects in a bucket, optionally under a prefix, following continuation tokens until the listing ends or `max_objects` is reached.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `bucket` | string | Yes | env `S3_BUCKET` | Bucket name or MRAP ARN; supports `${ctx.key}` interpolation. |
| `prefix` | string | No | `""` | Key prefix filter; supports `${ctx.key}` interpolation. |
| `delimiter` | string | No | `""` | Groups keys into `{output_key}_common_prefixes`, for example `"/"`. |
| `max_keys` | number | No | `1000` | Entries requested per page; a page size, not a bound on the listing. |
| `max_objects` | number | No | — | Stop after this many entries and return a continuation token; 1 to `IRONFLOW_MAX_S3_LIST_OBJECTS`. |
| `continuation_token` | string | No | — | Resume from an earlier `{output_key}_next_continuation_token`; an empty string starts from the beginning. |
| `region` | string | No | env `S3_REGION`, `AWS_REGION`, `AWS_DEFAULT_REGION` | Region override. |
| `endpoint_url` | string | No | env `AWS_ENDPOINT_URL` | Endpoint of an S3-compatible service. |
| `force_path_style` | boolean | No | env `AWS_S3_FORCE_PATH_STYLE`, else `false` | Use path-style bucket addressing. |
| `output_key` | string | No | `"s3"` | Prefix for the context output keys. |

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}_bucket` | string | Bucket listed. |
| `{output_key}_prefix` | string | Prefix used. |
| `{output_key}_count` | number | Objects returned by this call. |
| `{output_key}_objects` | array | Object entries; see Output Shape. |
| `{output_key}_common_prefixes` | array | Prefix strings grouped by `delimiter`; empty without one. |
| `{output_key}_truncated` | boolean | `true` when the call stopped at `max_objects` and more entries remain. |
| `{output_key}_next_continuation_token` | string | Token for the next call when truncated, otherwise `""`. |
| `{output_key}_success` | boolean | `true` on success. |

## Output Shape

One entry of `{output_key}_objects`:

```json
{"key": "raw/a.txt", "size": 1024, "etag": "\"9b2c...\"", "last_modified": "2026-01-05T10:00:00Z", "storage_class": "STANDARD"}
```

A field the service does not return is `""` (or `0` for `size`).

## Behavior

- Objects and common prefixes both count as entries toward `max_objects` and the limit.
- Without `max_objects`, a listing with more entries than `IRONFLOW_MAX_S3_LIST_OBJECTS` fails the step; it is never returned in part.
- With `max_objects`, pass `{output_key}_next_continuation_token` as `continuation_token` to the next call; it continues exactly after the last entry returned.
- The step fails when the endpoint returns a truncated page without a token, repeats a token, returns more entries than requested, or returns 16 truncated empty pages in a row.
- Credentials come from the AWS SDK default chain. An MRAP ARN is signed with SigV4a; see [S3 signing](../SECURITY.md#s3-signing).
- With `IRONFLOW_BLOCK_PRIVATE_NETWORK=true`, a private `endpoint_url` must be allow-listed; see [Outbound destinations](../SECURITY.md#outbound-destinations).

## Limits

| Variable | Default | Effect |
|----------|---------|--------|
| `IRONFLOW_MAX_S3_LIST_OBJECTS` | 10,000 | Maximum entries one call returns, and the upper bound for `max_objects`. |
| `IRONFLOW_MAX_S3_LIST_PAGE_BYTES` | 16 MiB | Maximum response body of one page; a larger page fails the step. |

## Examples

### List a prefix

```lua
local flow = Flow.new("s3_list")

flow:step("list", nodes.s3_list_objects({
    bucket = env("S3_BUCKET"),
    prefix = "raw/temp/notes/",
    max_keys = 50,
    output_key = "notes_list"
}))

flow:step("log", nodes.log({
    message = "Found ${ctx.notes_list_count} objects under ${ctx.notes_list_prefix}"
})):depends_on("list")

return flow
```

### Page through a large prefix

```lua
flow:step("first", nodes.s3_list_objects({
    bucket = env("S3_BUCKET"),
    prefix = "raw/",
    max_objects = 500,
    output_key = "first"
}))

-- An empty token lists from the beginning, so this step needs no condition;
-- check `first_truncated` before using its result.
flow:step("second", nodes.s3_list_objects({
    bucket = env("S3_BUCKET"),
    prefix = "raw/",
    max_objects = 500,
    continuation_token = "${ctx.first_next_continuation_token}",
    output_key = "second"
})):depends_on("first")
```
