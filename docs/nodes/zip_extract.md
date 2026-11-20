# `zip_extract`

Extract a ZIP archive into a destination directory after validating every entry.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | Yes | — | ZIP file to extract; supports `${ctx.key}` interpolation. |
| `destination` | string | Yes | — | Target directory; supports `${ctx.key}` interpolation. |
| `output_key` | string | No | `"extracted_files"` | Context key for the extracted entry names. |
| `overwrite` | boolean | No | `true` | When `false`, an existing target file fails the step. |
| `max_entries` | number | No | `IRONFLOW_MAX_ZIP_ENTRIES` | Maximum archive entries. |
| `max_metadata_bytes` | number | No | `IRONFLOW_MAX_ZIP_METADATA_BYTES` | Maximum total bytes of entry names, extra fields and comments. |
| `max_depth` | number | No | `IRONFLOW_MAX_DIRECTORY_DEPTH` | Maximum entry path depth (a top-level entry is depth 0). |
| `max_total_uncompressed_bytes` | number | No | `IRONFLOW_MAX_ZIP_UNCOMPRESSED_BYTES` | Maximum total uncompressed bytes, declared and actual. |

A zero or invalid limit override falls back to the default.

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}` | array | Entry names as stored in the archive, files and directories, in archive order. |
| `{output_key}_count` | number | Number of extracted entries. |
| `zip_extract_path` | string | Archive path after interpolation. |
| `zip_extract_destination` | string | Destination path after interpolation. |
| `zip_extract_success` | boolean | `true` after all entries are extracted. |

## Behavior

- Every entry is validated before the destination is changed. Absolute paths, `.` or `..`, empty components, backslashes, non-portable names (`:`, trailing `.` or space, `CON`, `NUL`, `COM1` and similar), symlink and special-file entries fail the step.
- Two entries that map to the same path ignoring case, or a file also used as a parent directory, fail the step.
- Malformed, multi-disk, or self-extracting archives (data before or after the ZIP records) fail the step.
- An entry cannot write through a symlink or special file in the destination; the destination root itself may be an alias such as macOS `/tmp`.
- Stored permission bits are ignored: on Unix a new file gets `0666` masked by the umask, and a replaced file keeps its bits. Set executable bits in a later step if needed.
- Missing directories are created as described in [Created directories](write_file.md#created-directories).
- Each file is published only when complete, but files from earlier entries remain if a later entry fails. Use a fresh destination when you need all-or-nothing extraction.

## Limits

| Variable | Default | Effect |
|----------|---------|--------|
| `IRONFLOW_MAX_ZIP_ENTRIES` | `10000` | Default for `max_entries`. |
| `IRONFLOW_MAX_ZIP_METADATA_BYTES` | 8 MiB | Default for `max_metadata_bytes`. |
| `IRONFLOW_MAX_DIRECTORY_DEPTH` | `32` | Default for `max_depth`. |
| `IRONFLOW_MAX_ZIP_UNCOMPRESSED_BYTES` | 512 MiB | Default for `max_total_uncompressed_bytes`. |

## Examples

### Extract with tight limits

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
