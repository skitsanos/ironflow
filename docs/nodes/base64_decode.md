# `base64_decode`

Decode a base64 string to text or write decoded bytes to a file.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `input` | string | one of `input` or `source_key` | — | Base64 string to decode; supports `${ctx.*}` interpolation. |
| `source_key` | string | see above | — | Context key containing the base64 string to decode. |
| `output_key` | string | no | `"base64_decoded"` | Context key for the decoded output. |
| `output_file` | string | no | — | File path to write decoded bytes to. |
| `url_safe` | bool | no | `false` | Expect URL-safe base64 alphabet. |

## Context Output

- If no `output_file`: `<output_key>` (default `base64_decoded`) — the decoded string.
- If `output_file` is set: `<output_key>_path` — the file path written to.

## Example

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
