# `s3_copy_object`

Copy an existing object to another key or bucket.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `source_bucket` | string | no | env `S3_BUCKET` | Source bucket name, not URL-encoded. Supports `${ctx.key}` interpolation. |
| `source_key` | string | yes | -- | Literal source object key, not a URL or pre-encoded path. Supports `${ctx.key}` interpolation. |
| `bucket` | string | no | env `S3_BUCKET` | Destination bucket name. |
| `key` | string | yes | -- | Destination object key. |
| `region` | string | no | `S3_REGION` / `AWS_REGION` | Explicit AWS/S3 region override. |
| `endpoint_url` | string | no | env `AWS_ENDPOINT_URL` | Optional custom endpoint (for S3-compatible services). |
| `force_path_style` | bool | no | `false` | Force path-style bucket addressing. |
| `output_key` | string | no | `"s3"` | Prefix for context output keys. |

## Source Key Encoding

IronFlow percent-encodes the source bucket/key path once before the AWS SDK
signs and sends `x-amz-copy-source`, as required by the
[S3 CopyObject contract](https://docs.aws.amazon.com/AmazonS3/latest/API/API_CopyObject.html#API_CopyObject_RequestSyntax).
Use the original key, just as for `s3_put_object` or `s3_get_object`:

- Spaces become `%20`, not `+`, and Unicode is encoded from its UTF-8 bytes.
- Literal percent signs are escaped: a key containing `%2F` is sent as `%252F`,
  not interpreted as a slash. Do not pre-encode input keys.
- Slashes and unreserved characters (`A-Z`, `a-z`, `0-9`, `-`, `.`, `_`, `~`)
  are retained. Repeated slashes and dot segments in the source key are not
  normalized.
- `?`, `#`, `&`, `=`, and `+` are key bytes, not query/fragment syntax. A key
  containing `?versionId=old` is copied literally. The node has no input version
  selector and copies the current source version; the optional
  `{output_key}_source_version_id` is response metadata, not a selector.

Context output keeps the original, unencoded bucket/key values. Destination
bucket/key encoding remains handled by the AWS SDK. Failed copies publish no
success output; service permissions, versioning, and copy constraints still
apply.

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

The runnable [S3 copy example](../../examples/04-file-operations/s3_copy.lua)
uploads a literal space/percent key, copies it, downloads and verifies its
contents, lists the run-owned prefix, and deletes both objects on success.
