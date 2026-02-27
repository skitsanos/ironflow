# `s3_list_objects`

List objects under a bucket and optional prefix.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `bucket` | string | yes | env `S3_BUCKET` | Bucket to query. |
| `prefix` | string | no | `""` | Prefix used to filter keys. |
| `delimiter` | string | no | `""` | Optional delimiter for grouped prefixes. |
| `max_keys` | number | no | -- | Optional maximum items per request/page. |
| `region` | string | no | `S3_REGION` / `AWS_REGION` | Explicit AWS/S3 region override. |
| `endpoint_url` | string | no | env `AWS_ENDPOINT_URL` | Optional custom endpoint (for S3-compatible services). |
| `force_path_style` | bool | no | `false` | Force path-style bucket addressing. |
| `output_key` | string | no | `"s3"` | Prefix for context output keys. |

## Context Output

- `{output_key}_bucket` — Queried bucket.
- `{output_key}_prefix` — Prefix filter used.
- `{output_key}_count` — Total objects returned (all pages).
- `{output_key}_objects` — Array of objects (`key`, `size`, `etag`, `last_modified`, `storage_class`).
- `{output_key}_success` — `true` on success.

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
