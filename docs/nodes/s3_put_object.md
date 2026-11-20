# `s3_put_object`

Upload text, Base64 data or a local file to S3 or an S3-compatible service.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `bucket` | string | Yes | env `S3_BUCKET` | Bucket name or MRAP ARN; supports `${ctx.key}` interpolation. |
| `key` | string | Yes | — | Object key; supports `${ctx.key}` interpolation. |
| `content` | string | One of three | — | Inline payload; supports `${ctx.key}` interpolation. |
| `source_key` | string | One of three | — | Context key holding the payload string. |
| `source_path` | string | One of three | — | Local file to upload as is; supports `${ctx.key}` interpolation. |
| `encoding` | string | No | `"text"` | How `content` or `source_key` is decoded: `text` or `base64`. Alias: `source_encoding`. |
| `content_type` | string | No | `"application/octet-stream"` | `Content-Type` stored with the object. |
| `region` | string | No | env `S3_REGION`, `AWS_REGION`, `AWS_DEFAULT_REGION` | Region override. |
| `endpoint_url` | string | No | env `AWS_ENDPOINT_URL` | Endpoint of an S3-compatible service. |
| `force_path_style` | boolean | No | env `AWS_S3_FORCE_PATH_STYLE`, else `false` | Use path-style bucket addressing. |
| `output_key` | string | No | `"s3"` | Prefix for the context output keys. |

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}_bucket` | string | Bucket written. |
| `{output_key}_key` | string | Object key written. |
| `{output_key}_content_type` | string | Content type sent. |
| `{output_key}_etag` | string | ETag, when returned. |
| `{output_key}_version_id` | string | Version ID, when the bucket is versioned. |
| `{output_key}_success` | boolean | `true` on success. |

## Behavior

- When several payload parameters are set, `source_path` wins over `source_key`, which wins over `content`. None of them fails the step.
- `encoding` does not apply to `source_path`. Invalid Base64, an unknown `encoding`, or a `source_key` value that is missing or not a string fails the step.
- Credentials come from the AWS SDK default chain. An MRAP ARN is signed with SigV4a; see [S3 signing](../SECURITY.md#s3-signing).
- With `IRONFLOW_BLOCK_PRIVATE_NETWORK=true`, a private `endpoint_url` must be allow-listed; see [Outbound destinations](../SECURITY.md#outbound-destinations).

## Limits

| Variable | Default | Effect |
|----------|---------|--------|
| `IRONFLOW_MAX_FILE_BYTES` | 50 MiB | Maximum size of a `source_path` file. |

## Examples

### Upload inline text

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
