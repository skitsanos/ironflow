# `s3_delete_object`

Delete one object from S3 or an S3-compatible service.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `bucket` | string | Yes | env `S3_BUCKET` | Bucket name or MRAP ARN; supports `${ctx.key}` interpolation. |
| `key` | string | Yes | — | Object key; supports `${ctx.key}` interpolation. |
| `version_id` | string | No | — | Version to delete in a versioned bucket. |
| `region` | string | No | env `S3_REGION`, `AWS_REGION`, `AWS_DEFAULT_REGION` | Region override. |
| `endpoint_url` | string | No | env `AWS_ENDPOINT_URL` | Endpoint of an S3-compatible service. |
| `force_path_style` | boolean | No | env `AWS_S3_FORCE_PATH_STYLE`, else `false` | Use path-style bucket addressing. |
| `output_key` | string | No | `"s3"` | Prefix for the context output keys. |

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}_bucket` | string | Bucket used. |
| `{output_key}_key` | string | Object key deleted. |
| `{output_key}_delete_marker` | boolean | Whether a delete marker was involved, when the service reports it. |
| `{output_key}_success` | boolean | `true` on success. |

## Behavior

- Credentials come from the AWS SDK default chain. An MRAP ARN is signed with SigV4a; see [S3 signing](../SECURITY.md#s3-signing).
- With `IRONFLOW_BLOCK_PRIVATE_NETWORK=true`, a private `endpoint_url` must be allow-listed; see [Outbound destinations](../SECURITY.md#outbound-destinations).

## Examples

### Delete an object

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
