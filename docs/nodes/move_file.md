# `move_file`

Move (rename) a file to a new path, creating missing parent directories.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `source` | string | Yes | — | Path to the file to move; supports `${ctx.key}` interpolation. |
| `destination` | string | Yes | — | New path for the file; supports `${ctx.key}` interpolation. |

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `move_file_source` | string | Source path after interpolation. |
| `move_file_destination` | string | Destination path after interpolation. |
| `move_file_success` | boolean | `true` after the move completes. |

## Behavior

- Source and destination must be on the same filesystem; a cross-device move fails the step instead of copying.
- The source is renamed as is: a source symlink moves the link itself.
- A destination that is a symlink, dangling link or special file fails the step; an existing regular file is replaced atomically.
- Missing parent directories are created as described in [Created directories](write_file.md#created-directories).

## Examples

### Rename a file

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
