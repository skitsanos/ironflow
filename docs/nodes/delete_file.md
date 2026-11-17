# `delete_file`

Delete a file.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | Yes | — | Path to the file to delete; supports `${ctx.key}` interpolation. |

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `delete_file_path` | string | Path after interpolation. |
| `delete_file_success` | boolean | `true` after the file is deleted. |

## Behavior

- A missing file or a directory fails the step.
- A symlink path removes the link, not its target.

## Examples

### Delete a file

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
