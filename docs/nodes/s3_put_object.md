# `s3_put_object`

Upload text or base64 payloads to S3 (or S3-compatible storage).

Uses regional SigV4 signing, or SigV4a when `bucket` is an MRAP ARN;
see [S3 signing](../SECURITY.md#s3-signing).

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `bucket` | string | yes | env `S3_BUCKET` | Destination bucket name or MRAP ARN. |
| `key` | string | yes | -- | Destination object key. |
| `content` | string | no | -- | Inline text payload for upload. |
| `source_key` | string | no | -- | Read string payload from context key instead of `content`. |
| `source_path` | string | no | -- | Read upload payload from local file path. |
| `encoding` | string | no | `"text"` | `"text"` or `"base64"` for `content`/`source_key`. |
| `source_encoding` | string | no | `"text"` | Optional alias for `encoding`. |
| `content_type` | string | no | `"application/octet-stream"` | Optional `Content-Type` metadata. |
| `region` | string | no | `S3_REGION` / `AWS_REGION` | Explicit AWS/S3 region override. |
| `endpoint_url` | string | no | env `AWS_ENDPOINT_URL` | Optional custom endpoint (for S3-compatible services). |
| `force_path_style` | bool | no | `false` | Force path-style bucket addressing. |
| `output_key` | string | no | `"s3"` | Prefix for context output keys. |

One of `content`, `source_key`, or `source_path` is required.

## Context Output

- `{output_key}_bucket` — Target bucket.
- `{output_key}_key` — Object key written.
- `{output_key}_content_type` — Effective content type used.
- `{output_key}_etag` — Optional response ETag.
- `{output_key}_version_id` — Optional response version id.
- `{output_key}_success` — `true` on success.

## Outbound destination policy

When the operator sets `IRONFLOW_BLOCK_PRIVATE_NETWORK=true`, `endpoint_url`
(or `AWS_ENDPOINT_URL`) is refused when it is `localhost` or not a public
unicast address, and every DNS answer of the service client is validated
and pinned. An S3-compatible endpoint on a private address must be listed by
the operator in `IRONFLOW_PRIVATE_NETWORK_ALLOWLIST` (for example
`minio.internal:9000`). See
[Outbound destinations](../SECURITY.md#outbound-destinations).

## Example

```lua
local flow = Flow.new("s3_upload")

flow:step("upload", nodes.s3_put_object({
    bucket = env("S3_BUCKET"),
    key = "raw/temp/notes/sample.txt",
    content = "Hello from IronFlow",
    output_key = "upload"
}))

flow:step("show", nodes.log({
    message = "Uploaded to ${ctx.upload_bucket}/${ctx.upload_key}"
})):depends_on("upload")

return flow
```
