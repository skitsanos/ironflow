# `read_file`

Read a regular file as UTF-8 text or Base64, or stream it into the content-addressed artifact store.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | Yes | — | Path to a regular file; supports `${ctx.key}` interpolation. |
| `output_key` | string | No | `"file"` | Prefix for the context keys written by this node. |
| `encoding` | string | No | `"text"` | `"text"` (UTF-8), `"base64"` (encoded string in context), or `"artifact"` (bytes go to the artifact store, not context). |
| `mime_type` | string | No | — | Media type saved in the artifact descriptor; only valid with `encoding = "artifact"`, 1 to 255 visible ASCII bytes. |

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}_content` | string | File contents for `text` or `base64`; absent in artifact mode. |
| `{output_key}_artifact` | object | `artifact_uri`, `sha256`, `size_bytes`, optional `mime_type`; artifact mode only. |
| `{output_key}_path` | string | The path after interpolation. |
| `{output_key}_success` | boolean | `true` on success. |

## Behavior

- Directories, FIFOs, devices and final-path symlinks fail the step.
- `text` fails the step when the file is not valid UTF-8.
- `mime_type` without `encoding = "artifact"` fails the step.
- Artifact mode stores content under `IRONFLOW_ARTIFACT_DIR` (default `data/artifacts`) or the S3 backend; see [`docs/CLI_REFERENCE.md`](../CLI_REFERENCE.md).
- Prefer `artifact` over `base64` for large binaries; Base64 keeps the whole encoded string in context.

## Limits

| Variable | Default | Effect |
|----------|---------|--------|
| `IRONFLOW_MAX_FILE_BYTES` | 50 MiB | Maximum bytes read from the file in any mode. |

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
