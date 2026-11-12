# `image_convert`

Convert between image formats (e.g. PNG to JPEG). Output format is inferred from the output file extension.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | one of `path` or `source_key` | — | Input image path (supports `${ctx.key}` interpolation) |
| `source_key` | string | one of `path` or `source_key` | — | Context key containing a source path, artifact URI/descriptor, `{ artifact = ... }`, or explicit Base64 object |
| `output_path` | string | yes | — | Output image path (format inferred from extension) |
| `quality` | number | no | `85` | JPEG quality (1-100), only used for JPEG output |
| `output_key` | string | no | `"image_convert"` | Prefix for output values |

Artifact inputs are opened and SHA-256 verified inside the tracked blocking
worker; decoding consumes that same rewound handle rather than a resolved store
pathname.

Supported source formats are BMP, Farbfeld, GIF, HDR, ICO, JPEG, PNG, PNM,
QOI, TGA, TIFF, and WebP. Output remains restricted to PNG or JPEG.

## Context Output

- `<output_key>_path` — output file path
- `<output_key>_format` — output format (from extension)
- `<output_key>_success` — `true` on success

## Example

```lua
local flow = Flow.new("image_convert_demo")

flow:step("convert", nodes.image_convert({
    path = "examples/fixtures/ironflow-sample.png",
    output_path = "output/photo.jpg",
    quality = 90
}))

flow:step("log", nodes.log({
    message = "Converted to: ${ctx.image_convert_path} (${ctx.image_convert_format})"
})):depends_on("convert")

return flow
```

## Destination contract

`output_path` is published through the same rooted, atomic writer as
[`write_file`](write_file.md), inside the tracked blocking worker: the
converted image is encoded into a staged file beside the destination, flushed
and synchronized, then renamed over it.

- The destination directory must already exist. The node creates no
  directories and fails with `image_convert: destination directory '<dir>'
  does not exist`. Directory aliases, including macOS `/tmp`, are resolved
  first.
- A destination that is a symlink (dangling or not), FIFO, device or directory
  is refused and its target is left untouched. An existing regular file is
  replaced.
- An encoder or write failure, an expired step deadline or cancellation
  removes the staged file and leaves an existing destination unchanged.
- On Unix the published file has mode `0600` (owner read and write only),
  whatever the umask and whatever mode the replaced file had; widen it in a
  later step when another user must read it. Other platforms use their native
  permission semantics. This is the staged writer's mode, under review in
  [IF-185](../issues/IF-185.md).
- Unix pins the resolved directory and stages relative to that handle. Other
  platforms revalidate the observed path immediately before an atomic
  replacement and cannot close a hostile parent-directory swap race; protect
  destination trees from same-identity mutation.

## Resource contract

Image headers and decoded byte requirements are checked before allocation using
`IRONFLOW_MAX_IMAGE_ENCODED_BYTES` (50 MiB), `IRONFLOW_MAX_IMAGE_PIXELS` (25
million), and `IRONFLOW_MAX_IMAGE_DECODE_ALLOCATION_BYTES` (128 MiB). JPEG
conversion admits the retained source plus RGB conversion buffer. Decode,
format conversion, and encode run on a tracked blocking worker with cancellation
checkpoints between opaque codec operations. JPEG output is streamed to the
staged file instead of first accumulating the complete encoded file in memory.
