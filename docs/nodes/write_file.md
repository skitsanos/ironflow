# `write_file`

Write content to a file, creating it if it does not exist. Supports both text and binary output.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | yes | — | Destination file path. Supports `${ctx.*}` interpolation. |
| `content` | string | no | `""` | The text to write. Supports `${ctx.*}` interpolation. Ignored when `source_key` is set. |
| `source_key` | string | no | — | Context key whose string value supplies the content. Use with `encoding = "base64"` to write binary data from context. |
| `encoding` | string | no | `"text"` | `"text"` writes UTF-8 bytes. `"base64"` decodes the content from base64 before writing (produces binary output). |
| `append` | bool | no | `false` | When `true`, content is appended to the file instead of overwriting it. The file is created if it does not exist. |

> When `source_key` is provided, the node reads the value from the workflow context instead of using `content`. This is useful for writing data produced by earlier steps (e.g., a base64-encoded image from an HTTP response).

## Context Output

- `write_file_path` — The resolved file path (after interpolation).
- `write_file_success` — `true` when the write completed successfully.

## Examples

### Write a text file

```lua
local flow = Flow.new("write_demo")

flow:step("write", nodes.write_file({
    path = "/tmp/ironflow_test.txt",
    content = "Hello from IronFlow!\nTimestamp: ${ctx.timestamp}"
}))

flow:step("append", nodes.write_file({
    path = "/tmp/ironflow_test.txt",
    content = "\nAppended line.",
    append = true
})):depends_on("write")

return flow
```

### Write a binary file from context

```lua
local flow = Flow.new("write_binary")

-- Assume a previous step stored base64 image data in ctx.img_data
flow:step("save_image", nodes.write_file({
    path = "/tmp/output.png",
    source_key = "img_data",
    encoding = "base64"
}))

return flow
```
