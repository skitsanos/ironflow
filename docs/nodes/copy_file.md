# `copy_file`

Copy a regular file to a new location through the same tracked atomic writer
as `write_file`. The copy runs on a tracked blocking worker and is bounded by
`IRONFLOW_MAX_FILE_BYTES` (default 50 MiB).

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `source` | string | yes | — | Path to the source file. Must be a regular file. Supports `${ctx.key}` interpolation. |
| `destination` | string | yes | — | Path for the copied file. Missing parent directories are created. Supports `${ctx.key}` interpolation. |

## Source and destination safety

- The source follows the `read_file` policy: FIFOs, devices, directories and,
  on Unix, a final-path symlink are refused before any byte is read. Its size
  is admitted against `IRONFLOW_MAX_FILE_BYTES`, and a source that changes
  size while being copied fails the copy.
- Configured parent-directory aliases, including macOS `/tmp`, are resolved
  before staging. Unix pins the resolved directory, so retargeting the alias
  afterward cannot redirect the copy. This is not permission to follow a
  destination-file symlink.
- A destination-file symlink, dangling link or special file is refused and its
  target is never opened or truncated. An existing regular destination is
  replaced only after a complete, flushed and synced copy.
- The copy keeps the source's permission bits (for example an executable
  `0751` script stays `0751`), like a plain `cp`; the staged file is created
  `0600` and receives the source mode before it is published.
- Source, admission, cancellation or commit failure leaves an existing
  destination unchanged and removes the staged file.
- Portable platforms revalidate the destination immediately before an OS-level
  atomic replacement, but cannot close a hostile parent-directory swap race;
  protect destination trees from same-identity mutation.

## Context Output

- `copy_file_source` — The resolved source path (after interpolation).
- `copy_file_destination` — The resolved destination path (after interpolation).
- `copy_file_success` — `true` when the staged copy was committed.

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
