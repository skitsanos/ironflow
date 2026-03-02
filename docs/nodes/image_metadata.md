# `image_metadata`

Extract metadata from an image file (dimensions, format, color type).

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | one of `path` or `source_key` | — | Source image path (supports `${ctx.*}` interpolation) |
| `source_key` | string | one of `path` or `source_key` | — | Context key containing a source path |
| `output_key` | string | no | `"image_metadata"` | Prefix for output values |

## Context Output

- `<output_key>_width` — image width in pixels
- `<output_key>_height` — image height in pixels
- `<output_key>_format` — detected format from file extension (e.g. `png`, `jpeg`)
- `<output_key>_color_type` — color type (e.g. `Rgb8`, `Rgba8`)

## Example

```lua
local flow = Flow.new("image_metadata_demo")

flow:step("meta", nodes.image_metadata({
    path = "data/samples/photo.png",
    output_key = "img"
}))

flow:step("log", nodes.log({
    message = "Image: ${ctx.img_width}x${ctx.img_height} (${ctx.img_format})"
})):depends_on("meta")

return flow
```
