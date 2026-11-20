# `copy_file`

Copy a regular file to a new path through the same atomic writer as `write_file`, creating missing parent directories.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `source` | string | Yes | — | Path to a regular source file; supports `${ctx.key}` interpolation. |
| `destination` | string | Yes | — | Path for the copy; supports `${ctx.key}` interpolation. |

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `copy_file_source` | string | Source path after interpolation. |
| `copy_file_destination` | string | Destination path after interpolation. |
| `copy_file_success` | boolean | `true` after the copy is committed. |

## Behavior

- A source that is a directory, FIFO, device or symlink fails the step.
- A destination that is a symlink, dangling link or special file fails the step; an existing regular file is replaced.
- A source that changes size during the copy fails the step.
- Any failure leaves an existing destination unchanged.
- On Unix the copy keeps the source's permission bits, whether the destination is new or replaced.
- Missing parent directories are created as described in [Created directories](write_file.md#created-directories).
- Copies and `write_file` writes to the same destination run one after another within one process.

## Limits

| Variable | Default | Effect |
|----------|---------|--------|
| `IRONFLOW_MAX_FILE_BYTES` | 50 MiB | Maximum size of the source file. |

## Examples

### Copy a file

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
