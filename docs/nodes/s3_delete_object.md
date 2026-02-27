# `s3_delete_object`

Delete a single object from S3 (or S3-compatible storage).

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `bucket` | string | yes | env `S3_BUCKET` | Target bucket name. |
| `key` | string | yes | -- | Object key to delete. |
| `version_id` | string | no | -- | Optional version ID for versioned buckets. |
| `region` | string | no | `S3_REGION` / `AWS_REGION` | Explicit AWS/S3 region override. |
| `endpoint_url` | string | no | env `AWS_ENDPOINT_URL` | Optional custom endpoint (for S3-compatible services). |
| `force_path_style` | bool | no | `false` | Force path-style bucket addressing. |
| `output_key` | string | no | `"s3"` | Prefix for context output keys. |

## Context Output

- `{output_key}_bucket` — Target bucket.
- `{output_key}_key` — Deleted object key.
- `{output_key}_delete_marker` — Whether delete marker was returned.
- `{output_key}_success` — `true` on success.

## Example

```lua
local flow = Flow.new("s3_delete")

flow:step("remove", nodes.s3_delete_object({
    bucket = env("S3_BUCKET"),
    key = "raw/temp/notes/sample.txt",
    output_key = "removed"
}))

flow:step("log", nodes.log({
    message = "Deleted object marker: ${ctx.removed_delete_marker}"
})):depends_on("remove")

return flow
```
