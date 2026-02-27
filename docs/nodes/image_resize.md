# `image_resize`

Resize a single image file.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | one of `path` or `source_key` | — | Source image path (supports `${ctx.*}` interpolation). |
| `source_key` | string | one of `path` or `source_key` | — | Context key containing either a path string, or an object entry `{ path = "..." }` / `{ base64 = "..." }`. |
| `output_path` | string | yes | — | Destination file path for the resized image. |
| `width` | number | no | — | Target width in px. Required if `height` is omitted. |
| `height` | number | no | — | Target height in px. Required if `width` is omitted. |
| `format` | string | no | inferred from `output_path` or `png` | Output format: `png`, `jpeg`, or `jpg`. |
| `output_key` | string | no | `"resized_image"` | Prefix for the generated context output keys. |

> If both `path` and `source_key` are provided, execution fails.

## Context Output

- `<output_key>` — output file path.
- `<output_key>_width` — output width in pixels.
- `<output_key>_height` — output height in pixels.
- `<output_key>_format` — output format (`"png"` or `"jpeg"`).
- `<output_key>_success` — `true` on success.

## Example

```lua
local flow = Flow.new("image_resize_demo")

flow:step("resize", nodes.image_resize({
    path = "data/samples/sample_front.png",
    output_path = "outputs/sample_front_small.png",
    width = 120
}))

flow:step("log", nodes.log({
    message = "Resized to ${ctx.resized_image_width}x${ctx.resized_image_height}"
})):depends_on("resize")

return flow
```
