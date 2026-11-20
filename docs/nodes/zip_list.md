# `zip_list`

List the entries of a ZIP archive without extracting it.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | Yes | — | ZIP file to list; supports `${ctx.key}` interpolation. |
| `output_key` | string | No | `"zip_entries"` | Context key for the entry array. |
| `max_entries` | number | No | `IRONFLOW_MAX_ZIP_ENTRIES` | Maximum archive entries. |
| `max_metadata_bytes` | number | No | `IRONFLOW_MAX_ZIP_METADATA_BYTES` | Maximum total bytes of entry names, extra fields and comments. |
| `max_total_uncompressed_bytes` | number | No | `IRONFLOW_MAX_ZIP_UNCOMPRESSED_BYTES` | Maximum total declared uncompressed size. |

A zero or invalid limit override falls back to the default.

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}` | array | One object per entry, in archive order. |
| `{output_key}_count` | number | Number of entries. |
| `zip_list_path` | string | Archive path after interpolation. |
| `zip_list_success` | boolean | `true` on success. |

## Output Shape

```json
[
  {
    "name": "docs/readme.txt",
    "is_directory": false,
    "size": 1024,
    "compressed_size": 512,
    "crc32": 3632233996,
    "method": "Deflated"
  }
]
```

`size` is the uncompressed size; `method` is the compression method name, such as `"Stored"` or `"Deflated"`.

## Behavior

- The archive must be a regular file; a symlink fails the step.
- Malformed, multi-disk, or self-extracting archives (data before or after the ZIP records) fail the step.
- Duplicate entry names fail the step.

## Limits

| Variable | Default | Effect |
|----------|---------|--------|
| `IRONFLOW_MAX_ZIP_ENTRIES` | `10000` | Default for `max_entries`. |
| `IRONFLOW_MAX_ZIP_METADATA_BYTES` | 8 MiB | Default for `max_metadata_bytes`. |
| `IRONFLOW_MAX_ZIP_UNCOMPRESSED_BYTES` | 512 MiB | Default for `max_total_uncompressed_bytes`. |

## Examples

### List an archive

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
