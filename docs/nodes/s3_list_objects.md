# `s3_list_objects`

List objects under a bucket and optional prefix.

Uses regional SigV4 signing, or SigV4a when `bucket` is an MRAP ARN;
see [S3 signing](../SECURITY.md#s3-signing).

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `bucket` | string | yes | env `S3_BUCKET` | Bucket name or MRAP ARN to query. |
| `prefix` | string | no | `""` | Prefix used to filter keys. |
| `delimiter` | string | no | `""` | Optional delimiter for grouped prefixes. |
| `max_keys` | number | no | `1000` | Entries requested per page. This is a page size, not a bound on the listing. |
| `max_objects` | number | no | -- | Stop after this many entries and return the token to continue from. Between 1 and `IRONFLOW_MAX_S3_LIST_OBJECTS`. |
| `continuation_token` | string | no | -- | Resume a listing from `{output_key}_next_continuation_token` of an earlier call. An empty string starts from the beginning. |
| `region` | string | no | `S3_REGION` / `AWS_REGION` | Explicit AWS/S3 region override. |
| `endpoint_url` | string | no | env `AWS_ENDPOINT_URL` | Optional custom endpoint (for S3-compatible services). |
| `force_path_style` | bool | no | `false` | Force path-style bucket addressing. |
| `output_key` | string | no | `"s3"` | Prefix for context output keys. |

## Context Output

- `{output_key}_bucket` — Queried bucket.
- `{output_key}_prefix` — Prefix filter used.
- `{output_key}_count` — Objects returned by this call (all pages it read).
- `{output_key}_objects` — Array of objects (`key`, `size`, `etag`, `last_modified`, `storage_class`).
- `{output_key}_common_prefixes` — Array of the prefixes grouped by `delimiter`; empty without one.
- `{output_key}_truncated` — `true` when the call stopped at `max_objects` and more entries remain.
- `{output_key}_next_continuation_token` — Token for the next call when truncated, otherwise `""`.
- `{output_key}_success` — `true` on success.

## Limits

One call returns at most `IRONFLOW_MAX_S3_LIST_OBJECTS` entries (default
`10000`); objects and common prefixes both count, as they do toward S3's
`max_keys`. The node follows continuation tokens itself, so the two cases
are:

- Without `max_objects`, the listing must fit the ceiling. A listing with
  more entries fails the step with `listing holds more than N entries`; it
  is never returned in part.
- With `max_objects`, the call stops at that many entries and reports
  `{output_key}_truncated = true` with the token to pass as
  `continuation_token` to the next call. The last page is requested with a
  smaller `max_keys`, so the token continues exactly after the last entry
  returned.

The step also fails, instead of requesting pages without end, when an
endpoint returns a truncated page without a continuation token, repeats a
token already used in the call, returns more entries than the page asked
for, or returns 16 consecutive truncated pages that hold no entry. These
shapes come from S3-compatible services reached through `endpoint_url`.
The response body of one page is read by the AWS SDK and is not bounded by
these limits.

## Example

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

Paging through a large prefix, 500 entries per call:

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
