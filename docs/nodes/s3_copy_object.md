# `s3_copy_object`

Copy an object to another key or bucket inside S3 or an S3-compatible service, without downloading it.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `source_bucket` | string | Yes | env `S3_BUCKET` | Source bucket name; supports `${ctx.key}` interpolation. |
| `source_key` | string | Yes | — | Source object key, not URL-encoded; supports `${ctx.key}` interpolation. |
| `bucket` | string | Yes | env `S3_BUCKET` | Destination bucket name or MRAP ARN; supports `${ctx.key}` interpolation. |
| `key` | string | Yes | — | Destination object key; supports `${ctx.key}` interpolation. |
| `region` | string | No | env `S3_REGION`, `AWS_REGION`, `AWS_DEFAULT_REGION` | Region override. |
| `endpoint_url` | string | No | env `AWS_ENDPOINT_URL` | Endpoint of an S3-compatible service. |
| `force_path_style` | boolean | No | env `AWS_S3_FORCE_PATH_STYLE`, else `false` | Use path-style bucket addressing. |
| `output_key` | string | No | `"s3"` | Prefix for the context output keys. |

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}_source_bucket` | string | Source bucket, as given. |
| `{output_key}_source_key` | string | Source key, as given. |
| `{output_key}_destination_bucket` | string | Destination bucket. |
| `{output_key}_destination_key` | string | Destination key. |
| `{output_key}_etag` | string | ETag of the new object, when returned. |
| `{output_key}_last_modified` | string | Timestamp of the copy, when returned. |
| `{output_key}_version_id` | string | Version ID of the new object, when returned. |
| `{output_key}_source_version_id` | string | Version ID of the copied source, when returned. |
| `{output_key}_expiration` | string | Expiration metadata, when returned. |
| `{output_key}_success` | boolean | `true` on success. |

## Behavior

- Pass the literal source key; do not pre-encode it. The node percent-encodes it once: a space becomes `%20`, and a key containing `%2F` stays a literal `%2F`, not a slash.
- `?`, `#`, `&`, `=` and `+` are part of the key. There is no source version selector; the current version is copied.
- An MRAP ARN works as the destination (signed with SigV4a), not as the source; see [S3 signing](../SECURITY.md#s3-signing).
- Credentials come from the AWS SDK default chain.
- With `IRONFLOW_BLOCK_PRIVATE_NETWORK=true`, a private `endpoint_url` must be allow-listed; see [Outbound destinations](../SECURITY.md#outbound-destinations).

## Examples

### Copy a key with literal special characters

```lua
local flow = Flow.new("s3_copy")

flow:step("copy", nodes.s3_copy_object({
    source_bucket = env("S3_BUCKET"),
    source_key = "raw/temp/notes/sample %2F.txt",
    bucket = env("S3_BUCKET"),
    key = "raw/temp/notes/sample-copy.txt",
    output_key = "copy"
}))

flow:step("log", nodes.log({
    message = "Copied ${ctx.copy_source_key} -> ${ctx.copy_destination_key}"
})):depends_on("copy")

return flow
```

The runnable [S3 copy example](../../examples/04-file-operations/s3_copy.lua) uploads, copies, verifies and cleans up.
