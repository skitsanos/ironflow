# `base64_encode`

Encode a string or file contents to base64.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `input` | string | one of `input`, `source_key`, or `file` | — | String to encode; supports `${ctx.*}` interpolation. |
| `source_key` | string | see above | — | Context key containing the string to encode. |
| `file` | string | see above | — | File path to read and encode. |
| `output_key` | string | no | `"base64_encoded"` | Context key for the encoded output. |
| `url_safe` | bool | no | `false` | Use URL-safe base64 alphabet. |

## Context Output

- `<output_key>` (default `base64_encoded`) — the base64 encoded string.

## Example

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
