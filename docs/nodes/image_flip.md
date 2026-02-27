# `image_flip`

Flip a single image horizontally or vertically.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | one of `path` or `source_key` | — | Source image path |
| `source_key` | string | one of `path` or `source_key` | — | Context key containing a source path/object |
| `output_path` | string | yes | — | Destination image path |
| `direction` | string | no | `"horizontal"` | `horizontal`, `vertical`, or `both` |
| `format` | string | no | inferred / `png` | `png` or `jpeg`/`jpg` |
| `output_key` | string | no | `"flipped_image"` | Prefix for output values |

## Context Output

- `<output_key>` — output file path
- `<output_key>_direction` — chosen direction
- `<output_key>_width` / `<output_key>_height`
- `<output_key>_format`
- `<output_key>_success`

## Example

```lua
local flow = Flow.new("image_flip_demo")

flow:step("flip", nodes.image_flip({
    path = "data/samples/sample_front.png",
    direction = "vertical",
    output_path = "output/sample_front_flip.png",
    output_key = "flipped"
}))

flow:step("log", nodes.log({
    message = "Flipped image file: ${ctx.flipped}"
})):depends_on("flip")

return flow
```

