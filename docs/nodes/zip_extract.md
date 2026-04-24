# `zip_extract`

Extract a ZIP archive into a destination directory.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | yes | — | Path to a ZIP file. Supports `${ctx.*}` interpolation. |
| `destination` | string | yes | — | Target directory for extracted files. Supports `${ctx.*}` interpolation. |
| `output_key` | string | no | `"extracted_files"` | Context key for extracted entry names. |
| `overwrite` | bool | no | `true` | When `false`, fail if a target file already exists. |
| `max_entries` | number | no | `IRONFLOW_MAX_ZIP_ENTRIES` / `10000` | Maximum archive entries extracted before failing. |
| `max_total_uncompressed_bytes` | number | no | `IRONFLOW_MAX_ZIP_UNCOMPRESSED_BYTES` / `536870912` | Maximum total uncompressed bytes extracted. |

## Context Output

- `{output_key}` — Array of extracted entry names (as stored in archive).
- `{output_key}_count` — Number of extracted entries.
- `zip_extract_path` — The resolved archive path.
- `zip_extract_destination` — The resolved destination directory.
- `zip_extract_success` — `true` when extraction completed successfully.

## Example

```lua
local flow = Flow.new("zip_extract_demo")

flow:step("extract", nodes.zip_extract({
    path = "/tmp/project_files.zip",
    destination = "/tmp/unpacked_project",
    output_key = "unpacked",
    overwrite = true
}))

flow:step("log", nodes.log({
    message = "Extracted ${ctx.unpacked_count} files into ${ctx.zip_extract_destination}",
    level = "info"
})):depends_on("extract")

return flow
```

## Limits

`zip_extract` rejects unsafe paths and enforces entry and total uncompressed byte caps before writing files. Raise the per-node limits only for archives you trust.
