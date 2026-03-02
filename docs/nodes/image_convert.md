# `image_convert`

Convert between image formats (e.g. PNG to JPEG). Output format is inferred from the output file extension.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | one of `path` or `source_key` | — | Input image path (supports `${ctx.*}` interpolation) |
| `source_key` | string | one of `path` or `source_key` | — | Context key containing a source path |
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
    path = "data/samples/photo.png",
    output_path = "output/photo.jpg",
    quality = 90
}))

flow:step("log", nodes.log({
    message = "Converted to: ${ctx.image_convert_path} (${ctx.image_convert_format})"
})):depends_on("convert")

return flow
```
