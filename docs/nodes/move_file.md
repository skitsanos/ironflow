# `move_file`

Move (rename) a file to a new location.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `source` | string | yes | — | Path to the file to move. Supports `${ctx.*}` interpolation. |
| `destination` | string | yes | — | New path for the file. Supports `${ctx.*}` interpolation. |

## Context Output

- `move_file_source` — The resolved source path (after interpolation).
- `move_file_destination` — The resolved destination path (after interpolation).
- `move_file_success` — `true` when the move completed successfully.

## Example

```lua
local flow = Flow.new("move_demo")

flow:step("move", nodes.move_file({
    source = "/tmp/old_name.txt",
    destination = "/tmp/new_name.txt"
}))

flow:step("done", nodes.log({
    message = "Moved to ${ctx.move_file_destination}",
    level = "info"
})):depends_on("move")

return flow
```
