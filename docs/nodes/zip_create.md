# `zip_create`

Create a ZIP archive from a file or a directory tree.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `source` | string | Yes | — | File or directory to archive; supports `${ctx.key}` interpolation. |
| `zip_path` | string | Yes | — | Output archive path; supports `${ctx.key}` interpolation. |
| `include_root` | boolean | No | `false` | Prefix entry names with the source directory name. |
| `compression` | string | No | `"deflated"` | `"stored"` or `"deflated"` (alias `"deflate"`). |
| `max_entries` | number | No | `IRONFLOW_MAX_ZIP_ENTRIES` | Maximum source files and directories visited. |
| `max_depth` | number | No | `IRONFLOW_MAX_DIRECTORY_DEPTH` | Maximum directory depth below the source root (root is depth 0). |
| `max_total_uncompressed_bytes` | number | No | `IRONFLOW_MAX_ZIP_UNCOMPRESSED_BYTES` | Maximum total source bytes. |

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `zip_create_path` | string | Archive path after interpolation. |
| `zip_create_source` | string | Source path after interpolation. |
| `zip_create_files` | number | Number of files added. |
| `zip_create_success` | boolean | `true` after the archive is published. |

## Behavior

- A symlink or special file at the source or anywhere below it fails the step; symlinks are never followed.
- A non-UTF-8 file name, or a name containing `\` or a NUL byte, fails the step.
- Only files are stored; empty directories are not.
- Entries are added in name order.
- Each entry records the source file's `rwx` permission bits (setuid, setgid, sticky and ownership are not stored). [`zip_extract`](zip_extract.md) ignores them.
- The archive is published only when complete; a failure leaves an existing archive unchanged. An existing output symlink fails the step.
- Missing parent directories of `zip_path` are created as described in [Created directories](write_file.md#created-directories).

## Limits

| Variable | Default | Effect |
|----------|---------|--------|
| `IRONFLOW_MAX_ZIP_ENTRIES` | `10000` | Default for `max_entries`. |
| `IRONFLOW_MAX_DIRECTORY_DEPTH` | `32` | Default for `max_depth`. |
| `IRONFLOW_MAX_ZIP_UNCOMPRESSED_BYTES` | 512 MiB | Default for `max_total_uncompressed_bytes`. |

## Examples

### Archive a directory

```lua
local flow = Flow.new("zip_create_demo")

flow:step("create", nodes.zip_create({
    source = "/tmp/project_files",
    zip_path = "/tmp/project_files.zip",
    include_root = true,
    compression = "deflated",
    max_entries = 100,
    max_depth = 8,
    max_total_uncompressed_bytes = 1048576
}))

flow:step("log", nodes.log({
    message = "Created archive: ${ctx.zip_create_path} with ${ctx.zip_create_files} files",
    level = "info"
})):depends_on("create")

return flow
```
