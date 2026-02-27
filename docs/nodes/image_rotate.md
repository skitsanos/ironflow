# `image_rotate`

Rotate a single image by 90-degree increments.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | one of `path` or `source_key` | — | Source image path |
| `source_key` | string | one of `path` or `source_key` | — | Context key containing a source path/object |
| `output_path` | string | yes | — | Destination image path |
| `angle` | number | no | `90` | One of `90`, `180`, `270` |
| `format` | string | no | inferred / `png` | `png` or `jpeg`/`jpg` |
| `output_key` | string | no | `"rotated_image"` | Prefix for output values |

## Context Output

- `<output_key>` — output file path
- `<output_key>_angle` — angle used
- `<output_key>_width` / `<output_key>_height` — output dimensions
- `<output_key>_source_width` / `<output_key>_source_height`
- `<output_key>_format`
- `<output_key>_success`

## Example

```lua
local flow = Flow.new("image_rotate_demo")

flow:step("rotate", nodes.image_rotate({
    path = "data/samples/sample_front.png",
    angle = 90,
    output_path = "output/sample_front_rotated.png",
    output_key = "rotated"
}))

flow:step("log", nodes.log({
    message = "Rotated image: ${ctx.rotated_width}x${ctx.rotated_height}"
})):depends_on("rotate")

return flow
```

