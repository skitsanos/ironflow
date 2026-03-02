# `image_watermark`

Overlay a semi-transparent watermark band on an image at a specified position.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | one of `path` or `source_key` | — | Input image path (supports `${ctx.*}` interpolation) |
| `source_key` | string | one of `path` or `source_key` | — | Context key containing a source path/object |
| `output_path` | string | yes | — | Output image path |
| `text` | string | no | `"watermark"` | Watermark text (supports `${ctx.*}` interpolation) |
| `position` | string | no | `"bottom-right"` | One of `bottom-right`, `bottom-left`, `top-right`, `top-left`, `center` |
| `opacity` | number | no | `0.5` | Opacity of the watermark band (0.0 - 1.0) |
| `format` | string | no | inferred / `png` | `png` or `jpeg`/`jpg` |
| `output_key` | string | no | `"image_watermark"` | Prefix for output values |

## Context Output

- `<output_key>_path` — output file path
- `<output_key>_text` — the watermark text applied
- `<output_key>_success` — `true` on success

## Example

```lua
local flow = Flow.new("image_watermark_demo")

flow:step("watermark", nodes.image_watermark({
    path = "data/samples/photo.png",
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
