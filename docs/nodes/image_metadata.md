# `image_metadata`

Extract metadata from an image file (dimensions, format, color type).

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | one of `path` or `source_key` | — | Source image path (supports `${ctx.key}` interpolation) |
| `source_key` | string | one of `path` or `source_key` | — | Context key containing a source path, artifact URI/descriptor, `{ artifact = ... }`, or explicit Base64 object |
| `output_key` | string | no | `"image_metadata"` | Prefix for output values |

## Context Output

- `<output_key>_width` — image width in pixels
- `<output_key>_height` — image height in pixels
- `<output_key>_format` — format detected from the image header (e.g. `png`, `jpeg`); artifact files do not need an extension
- `<output_key>_color_type` — color type (e.g. `Rgb8`, `Rgba8`)

## Example

```lua
local flow = Flow.new("image_metadata_demo")

flow:step("meta", nodes.image_metadata({
    path = "examples/fixtures/ironflow-sample.png",
    output_key = "img"
}))

flow:step("log", nodes.log({
    message = "Image: ${ctx.img_width}x${ctx.img_height} (${ctx.img_format})"
})):depends_on("meta")

return flow
```

## Resource contract

Metadata inspection sniffs the image header on a tracked blocking worker and
does not allocate a decoded pixel buffer. It still rejects encoded sources over
`IRONFLOW_MAX_IMAGE_ENCODED_BYTES` (50 MiB), dimensions over
`IRONFLOW_MAX_IMAGE_PIXELS` (25 million), or a decoder-reported byte requirement
over `IRONFLOW_MAX_IMAGE_DECODE_ALLOCATION_BYTES` (128 MiB).
