# `zip_extract`

Extract a ZIP archive into a destination directory.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | yes | — | Path to a ZIP file. Supports `${ctx.key}` interpolation. |
| `destination` | string | yes | — | Target directory for extracted files. Supports `${ctx.key}` interpolation. |
| `output_key` | string | no | `"extracted_files"` | Context key for extracted entry names. |
| `overwrite` | bool | no | `true` | When `false`, fail if a target file already exists. Supports `${ctx.key}` interpolation. |
| `max_entries` | number | no | `IRONFLOW_MAX_ZIP_ENTRIES` / `10000` | Maximum raw archive entries, checked before ZIP metadata construction. Supports `${ctx.key}` interpolation. |
| `max_metadata_bytes` | number | no | `IRONFLOW_MAX_ZIP_METADATA_BYTES` / `8388608` | Maximum cumulative raw filename, extra-field, file/archive-comment, and ZIP64 end-record extension bytes. Supports `${ctx.key}` interpolation. |
| `max_depth` | number | no | `IRONFLOW_MAX_DIRECTORY_DEPTH` / `32` | Maximum archive path depth. A top-level entry is depth 0. Supports `${ctx.key}` interpolation. |
| `max_total_uncompressed_bytes` | number | no | `IRONFLOW_MAX_ZIP_UNCOMPRESSED_BYTES` / `536870912` | Maximum total declared and actual uncompressed bytes extracted. Supports `${ctx.key}` interpolation. |

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
    overwrite = false,
    max_entries = 100,
    max_metadata_bytes = 1048576,
    max_depth = 8,
    max_total_uncompressed_bytes = 1048576
}))

flow:step("log", nodes.log({
    message = "Extracted ${ctx.unpacked_count} files into ${ctx.zip_extract_destination}",
    level = "info"
})):depends_on("extract")

return flow
```

## Preflight and filesystem safety

Before constructing ZIP library metadata, IronFlow scans raw single-disk
ZIP/ZIP64 headers, checking entry count, directory bounds, exact header lengths,
duplicate raw names, and the cumulative metadata budget. The end record must
terminate at EOF, including its archive comment. Unicode path aliases that
collapse during bounded ZIP construction also fail before destination changes.
The metadata budget does not cap compressed payload size or total process
memory; decoded names and library structures can require additional copies.
Zero or invalid overrides fall back to their environment/default values.

After this admission, and before mutating the destination, IronFlow checks every
archive entry's name, type, depth, duplicate/collision status, and declared
entry/byte limits. It
rejects absolute paths, `.` and `..`, empty components, backslashes,
non-portable path components, duplicate destinations, symlink entries, and
special-file entries. The archive itself must open as a regular file; on Unix,
a final symlink is rejected without being followed.

On Unix, archive-controlled traversal is pinned to opened directories and uses
directory-relative, no-follow operations for every parent and leaf. On other
platforms, IronFlow rejects symlinks observed during traversal, but the standard
library cannot provide the same race-free `openat` guarantee. An archive entry
cannot traverse a symlink or special-file destination component or leaf.

## Cancellation and partial output

Extraction runs on a tracked blocking worker and checks the enclosing step/run
deadline and cancellation signal between raw central headers, filename chunks,
entries, and copied chunks, and around the ZIP constructor. The synchronous
constructor cannot be interrupted midway. Each file
is staged beside its destination and published only after its complete contents
have been checked. Failure or cancellation removes the current temporary file
and preserves a prior destination leaf. Files and directories committed by
earlier entries may remain, so use a fresh run-owned destination when all-or-
nothing extraction is required. A failed or cancelled step publishes no partial
context output.
