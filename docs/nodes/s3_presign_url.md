# `s3_presign_url`

Generate a presigned URL (or presigned DELETE/HEAD/PUT URL) for S3 operations.

Uses regional SigV4 signing, or SigV4a when `bucket` is an MRAP ARN;
see [S3 signing](../SECURITY.md#s3-signing).

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `bucket` | string | yes | env `S3_BUCKET` | Regional bucket name or MRAP ARN used for signing. |
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

## Multi-Region Access Points

Use an ARN such as `arn:aws:s3::123456789012:accesspoint/fixture.mrap` for
`bucket`, with path-style addressing disabled and no custom endpoint. The
SDK generates an `AWS4-ECDSA-P256-SHA256` signature and a signing region set;
regional buckets continue to use `AWS4-HMAC-SHA256`. GET, PUT, HEAD and DELETE
are supported. Forward `{output_key}_headers` along with the URL when sending
the request, and keep the URL private until it expires.

The [MRAP presigning example](../../examples/04-file-operations/s3_mrap_presign.lua)
generates all four URL types without contacting S3. Static credentials make
signing network-free; other credential providers may contact their identity
service. Presigning does not verify that the MRAP exists or that its policy
permits the eventual request.

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
