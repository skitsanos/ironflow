# `s3_delete_object`

Delete a single object from S3 (or S3-compatible storage).

Uses regional SigV4 signing, or SigV4a when `bucket` is an MRAP ARN;
see [S3 signing](../SECURITY.md#s3-signing).

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `bucket` | string | yes | env `S3_BUCKET` | Target bucket name or MRAP ARN. |
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
