# `s3_get_object`

Download object content from S3 (or S3-compatible storage).

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `bucket` | string | yes | env `S3_BUCKET` | Destination bucket name. |
| `key` | string | yes | -- | Object key inside the bucket. |
| `region` | string | no | `S3_REGION` / `AWS_REGION` | Explicit AWS/S3 region override. |
| `endpoint_url` | string | no | env `AWS_ENDPOINT_URL` | Optional custom endpoint (for S3-compatible services). |
| `force_path_style` | bool | no | `false` | Force path-style bucket addressing. |
| `encoding` | string | no | `"text"` | `"text"` or `"base64"` for downloaded body output. |
| `output_key` | string | no | `"s3"` | Prefix for context output keys. |

## Context Output

- `{output_key}_bucket` — Bucket name used.
- `{output_key}_key` — Object key used.
- `{output_key}_content` — Object body (text or base64 depending on `encoding`).
- `{output_key}_encoding` — Body encoding used (`text` or `base64`).
- `{output_key}_size` — Byte size of the downloaded content.
- `{output_key}_content_type` — Optional `Content-Type` response header.
- `{output_key}_content_length` — Optional content length.
- `{output_key}_etag` — Optional object ETag.
- `{output_key}_last_modified` — Optional last modified timestamp.
- `{output_key}_success` — `true` on success.

## Example

```lua
local flow = Flow.new("s3_download")

flow:step("fetch", nodes.s3_get_object({
    bucket = env("S3_BUCKET"),
    key = "raw/temp/notes/sample.txt",
    output_key = "download"
}))

flow:step("log", nodes.log({
    message = "Fetched ${ctx.download_size} bytes from ${ctx.download_bucket}/${ctx.download_key}"
})):depends_on("fetch")

return flow
```
