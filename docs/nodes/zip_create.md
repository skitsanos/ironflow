# `zip_create`

Create a ZIP archive from a file or directory.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `source` | string | yes | — | File or directory path to archive. Supports `${ctx.key}` interpolation. |
| `zip_path` | string | yes | — | Output path for the generated ZIP file. Supports `${ctx.key}` interpolation. |
| `include_root` | bool | no | `false` | When `true`, prefix archived file names with the top-level source directory name. Supports `${ctx.key}` interpolation. Empty directories are not stored. |
| `compression` | string | no | `"deflated"` | Compression algorithm: `"stored"` (no compression) or `"deflated"`; `"deflate"` is accepted as an alias. |
| `max_entries` | number | no | `IRONFLOW_MAX_ZIP_ENTRIES` / `10000` | Maximum source entries visited. A directory source counts every child file and directory; a single-file source counts as one. Supports `${ctx.key}` interpolation. |
| `max_depth` | number | no | `IRONFLOW_MAX_DIRECTORY_DEPTH` / `32` | Maximum source-tree depth. The source root is depth 0; files directly inside it are allowed when the limit is 0. Supports `${ctx.key}` interpolation. |
| `max_total_uncompressed_bytes` | number | no | `IRONFLOW_MAX_ZIP_UNCOMPRESSED_BYTES` / `536870912` | Maximum total source bytes copied before compression. Supports `${ctx.key}` interpolation. |

## Context Output

- `zip_create_path` — The resolved output archive path.
- `zip_create_source` — The resolved source path.
- `zip_create_files` — Number of files added to the archive.
- `zip_create_success` — `true` when creation completed successfully.

## Example

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

## Filesystem, limits, and cancellation

The source must be a regular file or directory. IronFlow never follows source
symlinks: a symlink or special file at the root or anywhere below it fails the
step. Entry, depth, and actual copied-byte ceilings bound traversal and archive
work.

Creation runs on a tracked blocking worker and checks the enclosing step/run
deadline and cancellation signal while traversing entries and copying chunks.
The archive is written to a sibling temporary file. Failure or cancellation
removes that temporary file and preserves any prior output; only a complete
archive is published. An existing output symlink is rejected rather than
followed.
