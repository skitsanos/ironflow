# `read_file`

Read a file as text, explicitly encode it as Base64, or stream it into the disk-backed artifact store.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | yes | — | Path to a regular file. Supports `${ctx.key}` interpolation. |
| `output_key` | string | no | `"file"` | Prefix used for the context keys written by this node. |
| `encoding` | string | no | `"text"` | `"text"` reads UTF-8, `"base64"` explicitly places encoded bytes in context, and `"artifact"` streams bytes to the artifact store without materializing them in `NodeOutput`. |
| `mime_type` | string | no | — | Optional media type saved in the artifact descriptor. Valid only with `encoding = "artifact"`; when present it must be 1–255 visible ASCII bytes with no surrounding whitespace. |

## Context Output

- `{output_key}_content` — The file contents as a string for `text` or `base64`; absent in artifact mode.
- `{output_key}_artifact` — An object with `artifact_uri`, `sha256`, `size_bytes`, and optional `mime_type` in artifact mode; absent for inline encodings.
- `{output_key}_path` — The resolved file path (after interpolation).
- `{output_key}_success` — `true` when the file was read successfully.

## Resource and file-type limits

`read_file` accepts regular files only. FIFOs, devices, directories, and, on
Unix, final-path symlinks are rejected before their contents are read. Actual
bytes are additionally bounded by `IRONFLOW_MAX_FILE_BYTES` (50 MiB by
default), so a file that grows after its metadata check still cannot exceed the
configured raw-byte ceiling. Text and Base64 modes retain inline output in
memory; Base64 can temporarily coexist with its raw input and expanded encoded
string. Artifact mode instead copies in bounded chunks on a tracked worker,
hashes while copying, and atomically publishes immutable content under
`IRONFLOW_ARTIFACT_DIR` (default `data/artifacts`). The local store has no
automatic expiration; operators own retention and must provide a shared mount
when workflows can recover or resume on another host.

## Examples

### Read a text file

```lua
local flow = Flow.new("read_demo")

flow:step("read", nodes.read_file({
    path = "/tmp/ironflow_test.txt",
    output_key = "result"
}))

flow:step("show", nodes.log({
    message = "Read file successfully: ${ctx.result_success}",
    level = "info"
})):depends_on("read")

return flow
```

### Store a binary file without putting it in context

```lua
local flow = Flow.new("read_binary")

flow:step("read_img", nodes.read_file({
    path = "/tmp/photo.png",
    output_key = "image",
    encoding = "artifact",
    mime_type = "image/png"
}))

flow:step("show", nodes.log({
    message = "Stored ${ctx.image_artifact.size_bytes} bytes at ${ctx.image_artifact.artifact_uri}"
})):depends_on("read_img")

return flow
```

Use `encoding = "base64"` only when an external API specifically requires an
inline Base64 payload. Its raw bytes, encoded string, workflow value, and
persistence serialization can coexist temporarily, so it is not the
memory-stable handoff for large binaries.
