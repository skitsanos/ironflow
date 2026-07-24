# `s3_list_buckets`

List buckets visible to the current credentials.

This read-only node contacts an S3-compatible external service and uses the AWS
SDK credential chain plus the configured region/endpoint. On AWS S3 it requires
the account-level `s3:ListAllMyBuckets` permission. The runnable
[`s3_put_get_list.lua`](../../examples/04-file-operations/s3_put_get_list.lua)
workflow also creates and deletes a remote object. Its cleanup is a normal
success-dependent flow step, so a failed or interrupted run can leave the
object behind.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `region` | string | no | `S3_REGION` / `AWS_REGION` | Explicit AWS/S3 region override. |
| `endpoint_url` | string | no | env `AWS_ENDPOINT_URL` | Optional custom endpoint (for S3-compatible services). |
| `force_path_style` | bool | no | `false` | Force path-style bucket addressing. |
| `output_key` | string | no | `"s3"` | Prefix for context output keys. |

## Context Output

- `{output_key}_count` — Number of buckets returned.
- `{output_key}_buckets` — Array of buckets (`name`, `creation_date`).
- `{output_key}_success` — `true` on success.

## Example

See the linked workflow for the executable, cataloged example. Minimal use:

```lua
local flow = Flow.new("s3_buckets")

flow:step("buckets", nodes.s3_list_buckets({
    output_key = "s3cfg"
}))

flow:step("log", nodes.log({
    message = "Buckets found: ${ctx.s3cfg_count}"
})):depends_on("buckets")

return flow
```
