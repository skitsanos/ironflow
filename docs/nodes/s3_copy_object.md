# `s3_copy_object`

Copy an existing object to another key or bucket.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `source_bucket` | string | no | env `S3_BUCKET` | Source bucket name. |
| `source_key` | string | yes | -- | Source object key. |
| `bucket` | string | no | env `S3_BUCKET` | Destination bucket name. |
| `key` | string | yes | -- | Destination object key. |
| `region` | string | no | `S3_REGION` / `AWS_REGION` | Explicit AWS/S3 region override. |
| `endpoint_url` | string | no | env `AWS_ENDPOINT_URL` | Optional custom endpoint (for S3-compatible services). |
| `force_path_style` | bool | no | `false` | Force path-style bucket addressing. |
| `output_key` | string | no | `"s3"` | Prefix for context output keys. |

## Context Output

- `{output_key}_source_bucket` — Source bucket.
- `{output_key}_source_key` — Source key.
- `{output_key}_destination_bucket` — Destination bucket.
- `{output_key}_destination_key` — Destination key.
- `{output_key}_version_id` — Optional destination version id.
- `{output_key}_source_version_id` — Optional source version id.
- `{output_key}_etag` — Optional destination object ETag.
- `{output_key}_expiration` — Optional expiration metadata.
- `{output_key}_last_modified` — Optional copy operation timestamp.
- `{output_key}_success` — `true` on success.

## Example

```lua
local flow = Flow.new("s3_copy")

flow:step("copy", nodes.s3_copy_object({
    source_bucket = env("S3_BUCKET"),
    source_key = "raw/temp/notes/sample.txt",
    bucket = env("S3_BUCKET"),
    key = "raw/temp/notes/sample-copy.txt",
    output_key = "copy"
}))

flow:step("log", nodes.log({
    message = "Copied ${ctx.copy_source_key} -> ${ctx.copy_destination_key}"
})):depends_on("copy")

return flow
```
