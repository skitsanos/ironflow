# `image_watermark`

Overlay a semi-transparent watermark band on an image at a specified position.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | one of `path` or `source_key` | — | Input image path (supports `${ctx.key}` interpolation) |
| `source_key` | string | one of `path` or `source_key` | — | Context key containing a source path, artifact URI/descriptor, or explicit Base64 object |
| `output_path` | string | yes | — | Output image path |
| `text` | string | no | `"watermark"` | Watermark text (supports `${ctx.key}` interpolation) |
| `position` | string | no | `"bottom-right"` | One of `bottom-right`, `bottom-left`, `top-right`, `top-left`, `center` |
| `opacity` | number | no | `0.5` | Opacity of the watermark band (0.0 - 1.0) |
| `format` | string | no | inferred / `png` | `png` or `jpeg`/`jpg` |
| `output_key` | string | no | `"image_watermark"` | Prefix for output values |

Artifact inputs are opened and SHA-256 verified inside the tracked blocking
worker; decoding consumes that same rewound handle rather than a resolved store
pathname.

Supported source formats are BMP, Farbfeld, GIF, HDR, ICO, JPEG, PNG, PNM,
QOI, TGA, TIFF, and WebP. Output remains restricted to PNG or JPEG.

## Context Output

- `<output_key>_path` — output file path
- `<output_key>_text` — the watermark text applied
- `<output_key>_success` — `true` on success

## Example

```lua
local flow = Flow.new("image_watermark_demo")

flow:step("watermark", nodes.image_watermark({
    path = "examples/fixtures/ironflow-sample.png",
    output_path = "output/photo_watermarked.png",
    text = "CONFIDENTIAL",
    position = "bottom-right",
    opacity = 0.4
}))

flow:step("log", nodes.log({
    message = "Watermarked: ${ctx.image_watermark_path}"
})):depends_on("watermark")

return flow
```

## Resource contract

Image headers and decoded byte requirements are checked before allocation using
`IRONFLOW_MAX_IMAGE_ENCODED_BYTES` (50 MiB), `IRONFLOW_MAX_IMAGE_PIXELS` (25
million), and `IRONFLOW_MAX_IMAGE_DECODE_ALLOCATION_BYTES` (128 MiB). The
working estimate includes retained source plus RGBA output. Decode,
RGBA conversion, drawing, and encode run on a tracked blocking worker. The draw
loop checks cancellation periodically; codec operations remain opaque and are
checked immediately before and after they run.
