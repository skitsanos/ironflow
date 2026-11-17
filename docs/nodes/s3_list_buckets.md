# `s3_list_buckets`

List the buckets visible to the current credentials.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `region` | string | No | env `S3_REGION`, `AWS_REGION`, `AWS_DEFAULT_REGION` | Region override. |
| `endpoint_url` | string | No | env `AWS_ENDPOINT_URL` | Endpoint of an S3-compatible service. |
| `force_path_style` | boolean | No | env `AWS_S3_FORCE_PATH_STYLE`, else `false` | Use path-style bucket addressing. |
| `output_key` | string | No | `"s3"` | Prefix for the context output keys. |

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}_count` | number | Number of buckets. |
| `{output_key}_buckets` | array | Bucket entries; see Output Shape. |
| `{output_key}_success` | boolean | `true` on success. |

## Output Shape

One entry of `{output_key}_buckets`:

```json
{"name": "my-bucket", "creation_date": "2026-01-05T10:00:00Z"}
```

## Behavior

- On AWS the credentials need the `s3:ListAllMyBuckets` permission.
- Multi-Region Access Points are not listed.
- Credentials come from the AWS SDK default chain.
- With `IRONFLOW_BLOCK_PRIVATE_NETWORK=true`, a private `endpoint_url` must be allow-listed; see [Outbound destinations](../SECURITY.md#outbound-destinations).

## Limits

| Variable | Default | Effect |
|----------|---------|--------|
| `IRONFLOW_MAX_S3_LIST_PAGE_BYTES` | 16 MiB | Maximum response body; a larger response fails the step. |

## Examples

### Count buckets

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

The runnable [`s3_put_get_list.lua`](../../examples/04-file-operations/s3_put_get_list.lua) also uploads, reads and deletes an object.
