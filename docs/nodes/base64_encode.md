# `base64_encode`

Encode a string, a context value, or a file's bytes to base64.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `input` | string | One of | — | String to encode; supports `${ctx.key}` interpolation |
| `source_key` | string | One of | — | Context key holding the value to encode |
| `file` | string | One of | — | Path of a file to read and encode; supports `${ctx.key}` interpolation |
| `output_key` | string | No | `"base64_encoded"` | Context key for the encoded string |
| `url_safe` | bool | No | `false` | Use the URL-safe alphabet (`-` and `_`) |

Give exactly one of `input`, `source_key` or `file`; more than one, or none, fails the step.

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}` | string | The padded base64 string |

## Behavior

- A string under `source_key` is encoded as UTF-8 bytes; any other value is serialized to compact JSON first.
- A missing file, or a file larger than the limit below, fails the step.

## Limits

| Variable | Default | Effect |
|----------|---------|--------|
| `IRONFLOW_MAX_FILE_BYTES` | 50 MiB | Largest file `file` may read |

## Examples

### Encode a string

```lua
local flow = Flow.new("encode_demo")

flow:step("encode", nodes.base64_encode({
    input = "Hello, World!",
    output_key = "encoded"
}))

flow:step("log", nodes.log({
    message = "Encoded: ${ctx.encoded}"
})):depends_on("encode")

return flow
```
