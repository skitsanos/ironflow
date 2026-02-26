# `read_file`

Read the contents of a file into the workflow context. Supports both text and binary files.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | yes | — | Path to the file to read. Supports `${ctx.*}` interpolation. |
| `output_key` | string | no | `"file"` | Prefix used for the context keys written by this node. |
| `encoding` | string | no | `"text"` | `"text"` reads the file as a UTF-8 string. `"base64"` reads raw bytes and encodes them as a base64 string. |

## Context Output

- `{output_key}_content` — The file contents as a string (plain text or base64-encoded).
- `{output_key}_path` — The resolved file path (after interpolation).
- `{output_key}_success` — `true` when the file was read successfully.

## Examples

### Read a text file

```lua
local flow = Flow.new("read_demo")

flow:step("read", nodes.read_file({
    path = "/tmp/ironflow_test.txt",
    output_key = "result"
}))

flow:step("show", nodes.log({
    message = "File content: ${ctx.result_content}",
    level = "info"
})):depends_on("read")

return flow
```

### Read a binary file (image, PDF, etc.)

```lua
local flow = Flow.new("read_binary")

flow:step("read_img", nodes.read_file({
    path = "/tmp/photo.png",
    output_key = "image",
    encoding = "base64"
}))

flow:step("show", nodes.log({
    message = "Base64 length: ${ctx.image_content}"
})):depends_on("read_img")

return flow
```
