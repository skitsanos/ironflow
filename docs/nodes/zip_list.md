# `zip_list`

List entries in a ZIP archive.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | yes | — | Path to a ZIP file. Supports `${ctx.key}` interpolation. |
| `output_key` | string | no | `"zip_entries"` | Context key where the listing array is stored. |
| `max_entries` | number | no | `IRONFLOW_MAX_ZIP_ENTRIES` / `10000` | Maximum archive entries listed before failing. Supports `${ctx.key}` interpolation. |
| `max_total_uncompressed_bytes` | number | no | `IRONFLOW_MAX_ZIP_UNCOMPRESSED_BYTES` / `536870912` | Maximum total declared uncompressed entry size accepted while listing. Supports `${ctx.key}` interpolation. |

## Context Output

- `{output_key}` — An array of entries. Each entry has:
  - `name` — Entry path inside the archive.
  - `is_directory` — Whether entry is a directory.
  - `size` — Uncompressed size in bytes.
  - `compressed_size` — Compressed size in bytes.
  - `crc32` — CRC32 checksum.
  - `method` — Compression method used.
- `{output_key}_count` — Number of entries in the archive.
- `zip_list_path` — The resolved archive path.
- `zip_list_success` — `true` when listing completed successfully.

## Example

```lua
local flow = Flow.new("zip_list_demo")

flow:step("list", nodes.zip_list({
    path = "/tmp/project_files.zip",
    output_key = "entries"
}))

flow:step("log", nodes.log({
    message = "Archive entry count: ${ctx.entries_count}",
    level = "info"
})):depends_on("list")

return flow
```

## Filesystem, limits, and cancellation

The archive path must open as a regular file; on Unix, a final symlink is
rejected without being followed. `zip_list` validates entry count and total
declared uncompressed bytes while reading metadata.

Listing runs on a tracked blocking worker and checks the enclosing step/run
deadline and cancellation signal between entries. A failed or cancelled list
does not publish a partial array to workflow context.
