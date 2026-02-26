# `delete_file`

Delete a file.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | yes | — | Path to the file to delete. Supports `${ctx.*}` interpolation. |

## Context Output

- `delete_file_path` — The resolved file path (after interpolation).
- `delete_file_success` — `true` when the file was deleted successfully.

## Example

```lua
local flow = Flow.new("delete_demo")

flow:step("delete", nodes.delete_file({
    path = "/tmp/ironflow_test.txt"
}))

flow:step("done", nodes.log({
    message = "Deleted ${ctx.delete_file_path}",
    level = "info"
})):depends_on("delete")

return flow
```
