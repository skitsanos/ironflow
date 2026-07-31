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

## Resource contract

Image headers and decoded byte requirements are checked before allocation using
`IRONFLOW_MAX_IMAGE_ENCODED_BYTES` (50 MiB), `IRONFLOW_MAX_IMAGE_PIXELS` (25
million), and `IRONFLOW_MAX_IMAGE_DECODE_ALLOCATION_BYTES` (128 MiB). JPEG
conversion admits the retained source plus RGB conversion buffer. Decode,
format conversion, and encode run on a tracked blocking worker with cancellation
checkpoints between opaque codec operations. JPEG output is streamed to its
destination instead of first accumulating the complete encoded file in memory.
