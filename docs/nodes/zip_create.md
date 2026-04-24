# `zip_create`

Create a ZIP archive from a file or directory.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `source` | string | yes | — | File or directory path to archive. Supports `${ctx.*}` interpolation. |
| `zip_path` | string | yes | — | Output path for the generated ZIP file. Supports `${ctx.*}` interpolation. |
| `include_root` | bool | no | `false` | When `true`, include the top-level source directory name as the archive root entry when zipping a directory. |
| `compression` | string | no | `"deflated"` | Compression algorithm: `"stored"` (no compression) or `"deflated"`. |
| `max_entries` | number | no | `IRONFLOW_MAX_ZIP_ENTRIES` / `10000` | Maximum number of files added to the archive. |
| `max_total_uncompressed_bytes` | number | no | `IRONFLOW_MAX_ZIP_UNCOMPRESSED_BYTES` / `536870912` | Maximum total source bytes before compression. |

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
    compression = "deflated"
}))

flow:step("log", nodes.log({
    message = "Created archive: ${ctx.zip_create_path} with ${ctx.zip_create_files} files",
    level = "info"
})):depends_on("create")

return flow
```

## Limits

`zip_create` walks the input tree before writing the archive. It fails when the file count or total uncompressed source size exceeds the configured limits, which prevents unexpectedly large archives from consuming CPU, memory, or storage.
