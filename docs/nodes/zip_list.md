# `zip_list`

List all entries in a ZIP archive.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | yes | — | Path to a ZIP file. Supports `${ctx.*}` interpolation. |
| `output_key` | string | no | `"zip_entries"` | Context key where the listing array is stored. |

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
