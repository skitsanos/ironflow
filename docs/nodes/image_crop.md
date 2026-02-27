# `image_crop`

Crop a single image file.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | one of `path` or `source_key` | — | Source image path (supports `${ctx.*}` interpolation). |
| `source_key` | string | one of `path` or `source_key` | — | Context key containing either a path string, or an object entry `{ path = "..." }` / `{ base64 = "..." }`. |
| `output_path` | string | yes | — | Destination file path for the cropped image. |
| `x` | number | no | `0` | Left offset in pixels. |
| `y` | number | no | `0` | Top offset in pixels. |
| `crop_width` | number | no | alias: `width` | Crop width in pixels. |
| `crop_height` | number | no | alias: `height` | Crop height in pixels. |
| `format` | string | no | inferred from `output_path` or `png` | Output format: `png`, `jpeg`, or `jpg`. |
| `output_key` | string | no | `"cropped_image"` | Prefix for the generated context output keys. |

> If both `path` and `source_key` are provided, execution fails.

## Context Output

- `<output_key>` — output file path.
- `<output_key>_width` — crop width.
- `<output_key>_height` — crop height.
- `<output_key>_x` — x offset used.
- `<output_key>_y` — y offset used.
- `<output_key>_format` — output format (`"png"` or `"jpeg"`).
- `<output_key>_success` — `true` on success.

## Example

```lua
local flow = Flow.new("image_crop_demo")

flow:step("crop", nodes.image_crop({
    path = "data/samples/sample_front.png",
    output_path = "outputs/sample_front_cropped.png",
    x = 10,
    y = 8,
    crop_width = 120,
    crop_height = 80
}))

flow:step("log", nodes.log({
    message = "Cropped image: ${ctx.cropped_image_width}x${ctx.cropped_image_height}"
})):depends_on("crop")

return flow
```
