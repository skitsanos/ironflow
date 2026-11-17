# `s3_get_object`

Download an object from S3 or an S3-compatible service into the context as text or Base64.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `bucket` | string | Yes | env `S3_BUCKET` | Bucket name or MRAP ARN; supports `${ctx.key}` interpolation. |
| `key` | string | Yes | — | Object key; supports `${ctx.key}` interpolation. |
| `encoding` | string | No | `"text"` | Body encoding in the context: `text` or `base64`. |
| `region` | string | No | env `S3_REGION`, `AWS_REGION`, `AWS_DEFAULT_REGION` | Region override. |
| `endpoint_url` | string | No | env `AWS_ENDPOINT_URL` | Endpoint of an S3-compatible service. |
| `force_path_style` | boolean | No | env `AWS_S3_FORCE_PATH_STYLE`, else `false` | Use path-style bucket addressing. |
| `output_key` | string | No | `"s3"` | Prefix for the context output keys. |

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}_bucket` | string | Bucket used. |
| `{output_key}_key` | string | Object key used. |
| `{output_key}_content` | string | Object body, as text or Base64. |
| `{output_key}_encoding` | string | `"text"` or `"base64"`. |
| `{output_key}_size` | number | Body size in bytes. |
| `{output_key}_content_type` | string | `Content-Type` of the object, when returned. |
| `{output_key}_content_length` | number | Declared content length, when returned. |
| `{output_key}_etag` | string | Object ETag, when returned. |
| `{output_key}_last_modified` | string | Last-modified timestamp, when returned. |
| `{output_key}_success` | boolean | `true` on success. |

## Behavior

- With `encoding = "text"`, a body that is not valid UTF-8 fails the step; use `base64` for binary objects.
- An `encoding` other than `text` or `base64` fails the step after the download.
- Credentials come from the AWS SDK default chain. An MRAP ARN is signed with SigV4a; see [S3 signing](../SECURITY.md#s3-signing).
- With `IRONFLOW_BLOCK_PRIVATE_NETWORK=true`, a private `endpoint_url` must be allow-listed; see [Outbound destinations](../SECURITY.md#outbound-destinations).

## Limits

| Variable | Default | Effect |
|----------|---------|--------|
| `IRONFLOW_MAX_FILE_BYTES` | 50 MiB | Maximum object body size; a larger declared length or streamed body fails the step. |

## Examples

### Download a text object

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
