# `s3_presign_url`

Generate a presigned URL (or presigned DELETE/HEAD/PUT URL) for S3 operations.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `bucket` | string | yes | env `S3_BUCKET` | Bucket name used for signing. |
| `key` | string | yes | -- | Object key to access. |
| `method` | string | no | `"GET"` | One of `GET`, `PUT`, `HEAD`, `DELETE`. |
| `expires_in` | number | no | `3600` | URL validity in seconds (`1..604800`). |
| `content_type` | string | no | `"application/octet-stream"` | Optional content type for presigned `PUT`. |
| `content_length` | number | no | -- | Optional content length hint for presigned `PUT`. |
| `region` | string | no | `S3_REGION` / `AWS_REGION` | Explicit AWS/S3 region override. |
| `endpoint_url` | string | no | env `AWS_ENDPOINT_URL` | Optional custom S3-compatible endpoint. |
| `force_path_style` | bool | no | `false` | Force path-style bucket addressing. |
| `output_key` | string | no | `"s3"` | Prefix for context output keys. |

## Context Output

- `{output_key}_bucket` — Bucket used for signing.
- `{output_key}_key` — Object key used for signing.
- `{output_key}_method` — HTTP method used.
- `{output_key}_expires_in` — Expiration window in seconds.
- `{output_key}_url` — Presigned URL string.
- `{output_key}_headers` — Required headers for signing (if any).
- `{output_key}_success` — `true` on success.

## Example

```lua
local flow = Flow.new("s3_presign")

flow:step("setup", nodes.s3_put_object({
    bucket = env("S3_BUCKET"),
    key = "raw/temp/demo/presign.txt",
    content = "Hello from presign flow",
    output_key = "upload"
}))

flow:step("presign", nodes.s3_presign_url({
    bucket = env("S3_BUCKET"),
    key = "raw/temp/demo/presign.txt",
    method = "GET",
    expires_in = 600,
    output_key = "presigned"
})):depends_on("setup")

flow:step("read", nodes.http_get({
    url = "${ctx.presigned_url}",
    output_key = "download"
})):depends_on("presign")

flow:step("cleanup", nodes.s3_delete_object({
    bucket = env("S3_BUCKET"),
    key = "raw/temp/demo/presign.txt",
    output_key = "deleted"
})):depends_on("read")

flow:step("log", nodes.log({
    message = "Presigned URL generated (${ctx.presigned_expires_in}s): ${ctx.presigned_url}"
})):depends_on("cleanup")

return flow
```
