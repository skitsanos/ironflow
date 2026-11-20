# `base64_decode`

Decode a base64 string to UTF-8 text in context, or write the decoded bytes to a file.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `input` | string | One of | — | Base64 string; supports `${ctx.key}` interpolation |
| `source_key` | string | One of | — | Context key holding the base64 string |
| `output_key` | string | No | `"base64_decoded"` | Context key (or key prefix with `output_file`) for the result |
| `output_file` | string | No | — | Write the decoded bytes to this path instead of context; supports `${ctx.key}` interpolation |
| `url_safe` | bool | No | `false` | Expect the URL-safe alphabet (`-` and `_`) |

Give exactly one of `input` or `source_key`; both, or neither, fails the step.

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}` | string | The decoded text; set only without `output_file` |
| `{output_key}_path` | string | The path written; set only with `output_file` |

## Behavior

- Input must be padded base64 in the chosen alphabet; invalid input fails the step.
- Without `output_file`, decoded bytes that are not valid UTF-8 fail the step; use `output_file` for binary data.
- With `output_file`, the file is written like [`write_file`](write_file.md): missing parent directories are created ([permissions](write_file.md#created-directories)), an existing regular file is replaced only after a complete write, and a symlink or special file at the destination is refused.
- On malformed input, an oversized payload, cancellation or a write error, an existing file is left unchanged. File errors end with `; output_file '<path>'`.
- A decoded payload over the limit fails before anything is written: `base64_decode: final payload is <decoded> bytes, exceeds the IRONFLOW_MAX_FILE_BYTES limit (<limit>)`.

## Limits

| Variable | Default | Effect |
|----------|---------|--------|
| `IRONFLOW_MAX_FILE_BYTES` | 50 MiB | Largest decoded payload written with `output_file` |

## Examples

### Decode to text

```lua
local flow = Flow.new("decode_demo")

flow:step("decode", nodes.base64_decode({
    input = "SGVsbG8sIFdvcmxkIQ==",
    output_key = "decoded"
}))

flow:step("log", nodes.log({
    message = "Decoded: ${ctx.decoded}"
})):depends_on("decode")

return flow
```
