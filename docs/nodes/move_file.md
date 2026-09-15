# `move_file`

Move (rename) a file to a new location. The destination directory is prepared
with the shared rooted policy used by `write_file`, and the rename runs on a
tracked blocking worker.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `source` | string | yes | — | Path to the file to move. Supports `${ctx.key}` interpolation. |
| `destination` | string | yes | — | New path for the file. Missing parent directories are created. Supports `${ctx.key}` interpolation. |

## Destination safety

- Configured parent-directory aliases, including macOS `/tmp`, are resolved
  before the rename. Unix pins the resolved directory and renames relative to
  that handle, so retargeting the alias afterward cannot redirect the move.
  This is not permission to follow a destination-file symlink.
- A destination leaf that is a symlink, dangling link or special file is
  refused, so the move never replaces a link or writes through one. An
  existing regular destination file is replaced atomically.
- The source is renamed as-is: a source symlink moves the link itself, and the
  source must stay on the same filesystem. A cross-device move fails with an
  error rather than copying and deleting.
- Portable platforms revalidate the destination immediately before the rename,
  but cannot close a hostile parent-directory swap race; protect destination
  trees from same-identity mutation.

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
