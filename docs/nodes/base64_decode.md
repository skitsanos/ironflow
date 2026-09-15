# `base64_decode`

Decode a base64 string to text or write decoded bytes to a file.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `input` | string | one of `input` or `source_key` | — | Base64 string to decode; supports `${ctx.key}` interpolation. |
| `source_key` | string | see above | — | Context key containing the base64 string to decode. |
| `output_key` | string | no | `"base64_decoded"` | Context key for the decoded output. |
| `output_file` | string | no | — | File path to write decoded bytes to. |
| `url_safe` | bool | no | `false` | Expect URL-safe base64 alphabet. |

## Context Output

- If no `output_file`: `<output_key>` (default `base64_decoded`) — the decoded string.
- If `output_file` is set: `<output_key>_path` — the file path written to.

## File safety

`output_file` streams the decoded bytes through the same atomic, tracked write
path as `write_file`. The decoded length is derived from the encoded input and
admitted against `IRONFLOW_MAX_FILE_BYTES` (default 50 MiB) before any worker
or decoded-byte allocation. An oversized payload fails with exactly one
message:

```
base64_decode: final payload is <decoded> bytes, exceeds the IRONFLOW_MAX_FILE_BYTES limit (<limit>)
```

Decoding then runs in bounded chunks directly into the staged file, so the
decoded payload is never held in memory; only the text output path (no
`output_file`) decodes in memory. File errors name the destination once, as
`<error>; output_file '<path>'`.

Valid directory aliases (including macOS `/tmp`) are accepted, but destination
file symlinks and special files are refused. Missing parent directories are
created. An existing regular file is replaced only after a complete, synced
write; malformed input, byte-limit, cancellation or write failure preserves it
and removes temporary output.

Unix pins the resolved destination directory and uses handle-relative writes.
Other platforms retain observed-path checks and atomic replacement; protect
the parent namespace against concurrent hostile mutation. This does not impose
a filesystem sandbox on a trusted flow's configured destination.

## Example

```lua
local flow = Flow.new("decode_demo")

flow:step("decode", nodes.base64_decode({
    input = "SGVsbG8sIFdvcmxkIQ==",
    output_key = "decoded"
}))

flow:step("log", nodes.log({
    message = "Decoded: ${ctx.decoded}"
})):depends_on("decode")

return flow
```
