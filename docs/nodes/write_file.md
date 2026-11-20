# `write_file`

Atomically write text, Base64-decoded bytes, or a stored artifact to a regular file, creating missing parent directories.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | Yes | — | Destination path; supports `${ctx.key}` interpolation. |
| `content` | string | No | `""` | Inline text or Base64 input; supports interpolation. |
| `source_key` | string | No | — | Context key holding a string or artifact descriptor; the value is used as is, without interpolation. |
| `artifact` | object or string | No | — | Artifact descriptor or canonical `artifact://sha256/...` URI; a URI string supports interpolation. |
| `encoding` | string | No | `"text"` | `"text"`, `"base64"`, or `"artifact"`. |
| `append` | boolean | No | `false` | Write the existing file's content followed by the new input. |

`content`, `source_key` and `artifact` are mutually exclusive. Omitting all three writes an empty file.

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `write_file_path` | string | Destination path after interpolation. |
| `write_file_success` | boolean | `true` after the file is committed. |

## Behavior

- An object value is treated as an artifact without `encoding = "artifact"`; a canonical URI held as a context string needs `encoding = "artifact"`.
- An artifact input must be a descriptor, a canonical URI, or an `{ artifact = ... }` wrapper; a local file path fails the step. Use [`copy_file`](copy_file.md) to copy a file.
- Invalid Base64, an oversized result, cancellation or a write failure leaves an existing destination unchanged.
- A destination that is a symlink or not a regular file fails the step.
- `append` applies the size limit to the resulting file, not only to the new input.
- On Unix a new file gets `0666` masked by the umask; overwrite and append keep the existing file's permission bits (setuid, setgid and sticky are dropped).
- Writes to the same destination within one IronFlow process run one after another, so parallel appends keep every input. Separate processes writing the same file are last-writer-wins.

Accepted and rejected artifact forms are shown in [`write_file_artifact_inputs.lua`](../../examples/04-file-operations/write_file_artifact_inputs.lua). Artifact storage and backends are described in [binary artifacts](../ARCHITECTURE.md#binary-artifacts).

## Created directories

Missing parent directories are created by `write_file`, `copy_file`, `move_file`, `base64_decode` with `output_file`, `zip_create`, `zip_extract`, `pdf_merge`, `pdf_split`, `markdown_to_pdf`, `markdown_to_docx`, and `cache_set` with the file backend. On Unix each new directory gets `0755` masked by the umask (`0750` under `027`, `0700` under `077`), never group- or world-writable. An existing directory keeps its mode; create it beforehand if you need a different one.

## Limits

| Variable | Default | Effect |
|----------|---------|--------|
| `IRONFLOW_MAX_FILE_BYTES` | 50 MiB | Maximum size of the written file, including existing content in append mode. |

## Examples

### Write, append, and restore an artifact

```lua
local flow = Flow.new("write_demo")

flow:step("write", nodes.write_file({
    path = "/tmp/ironflow.txt",
    content = "Hello from IronFlow"
}))

flow:step("append", nodes.write_file({
    path = "/tmp/ironflow.txt",
    content = "\nAppended safely.",
    append = true
})):depends_on("write")

-- A previous node placed an artifact descriptor in ctx.cached_artifact.
flow:step("restore", nodes.write_file({
    path = "/tmp/restored.bin",
    source_key = "cached_artifact"
}))

return flow
```
