# `copy_file`

Copy a file to a new location.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `source` | string | yes | — | Path to the source file. Supports `${ctx.*}` interpolation. |
| `destination` | string | yes | — | Path for the copied file. Supports `${ctx.*}` interpolation. |

## Context Output

- `copy_file_source` — The resolved source path (after interpolation).
- `copy_file_destination` — The resolved destination path (after interpolation).
- `copy_file_success` — `true` when the copy completed successfully.

## Example

```lua
local flow = Flow.new("copy_demo")

flow:step("copy", nodes.copy_file({
    source = "/tmp/original.txt",
    destination = "/tmp/backup.txt"
}))

flow:step("done", nodes.log({
    message = "Copied to ${ctx.copy_file_destination}",
    level = "info"
})):depends_on("copy")

return flow
```
