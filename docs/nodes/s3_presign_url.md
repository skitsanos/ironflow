# `s3_presign_url`

Generate a presigned URL for a `GET`, `PUT`, `HEAD` or `DELETE` request on one object. No request is sent to S3.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `bucket` | string | Yes | env `S3_BUCKET` | Bucket name or MRAP ARN; supports `${ctx.key}` interpolation. |
| `key` | string | Yes | — | Object key; supports `${ctx.key}` interpolation. |
| `method` | string | No | `"GET"` | `GET`, `PUT`, `HEAD` or `DELETE`; case-insensitive. |
| `expires_in` | number | No | `3600` | URL validity in seconds, 1 to 604800 (7 days). |
| `content_type` | string | No | `"application/octet-stream"` | Content type signed into a `PUT` URL. |
| `content_length` | number | No | — | Content length signed into a `PUT` URL; alias `contentLength`. |
| `region` | string | No | env `S3_REGION`, `AWS_REGION`, `AWS_DEFAULT_REGION` | Region override. |
| `endpoint_url` | string | No | env `AWS_ENDPOINT_URL` | Endpoint of an S3-compatible service. |
| `force_path_style` | boolean | No | env `AWS_S3_FORCE_PATH_STYLE`, else `false` | Use path-style bucket addressing. |
| `output_key` | string | No | `"s3"` | Prefix for the context output keys. |

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}_bucket` | string | Bucket signed for. |
| `{output_key}_key` | string | Object key signed for. |
| `{output_key}_method` | string | Method, uppercased. |
| `{output_key}_expires_in` | number | Validity in seconds. |
| `{output_key}_url` | string | Presigned URL. |
| `{output_key}_headers` | object | Headers the request must send with the URL; may be empty. |
| `{output_key}_success` | boolean | `true` on success. |

## Behavior

- An unsupported `method` or an `expires_in` outside 1 to 604800 fails the step.
- A `PUT` URL is signed with `content_type` (and `content_length` when set), so the upload must send the same values; forward `{output_key}_headers` with the request.
- An MRAP ARN as `bucket` produces a SigV4a (`AWS4-ECDSA-P256-SHA256`) URL; use it without `endpoint_url` or path-style addressing. See [S3 signing](../SECURITY.md#s3-signing).
- Presigning does not check that the object or bucket exists or that the request will be allowed.
- Credentials come from the AWS SDK default chain; static credentials sign without any network call.
- With `IRONFLOW_BLOCK_PRIVATE_NETWORK=true`, a private `endpoint_url` must be allow-listed; see [Outbound destinations](../SECURITY.md#outbound-destinations).

## Examples

### Presign a download

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

The [MRAP presigning example](../../examples/04-file-operations/s3_mrap_presign.lua) generates all four URL types without contacting S3.
