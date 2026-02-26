# `list_directory`

List files and directories within a given path.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | yes | — | Directory to list. Supports `${ctx.*}` interpolation. |
| `recursive` | bool | no | `false` | When `true`, descend into subdirectories and include their entries as well. |
| `output_key` | string | no | `"files"` | Context key where the resulting array is stored. |

## Context Output

- `{output_key}` — A JSON array of entry objects. Each object contains:
  - `name` — File or directory name (string).
  - `type` — Either `"file"` or `"directory"`.
  - `path` — Absolute path to the entry (string).

## Example

```lua
local flow = Flow.new("list_demo")

flow:step("list", nodes.list_directory({
    path = "/tmp",
    recursive = false,
    output_key = "tmp_files"
}))

flow:step("show", nodes.log({
    message = "Found entries: ${ctx.tmp_files}",
    level = "info"
})):depends_on("list")

return flow
```
