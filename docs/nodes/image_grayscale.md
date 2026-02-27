# `image_grayscale`

Convert a single image to grayscale.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | one of `path` or `source_key` | — | Source image path |
| `source_key` | string | one of `path` or `source_key` | — | Context key containing a source path/object |
| `output_path` | string | yes | — | Destination image path |
| `format` | string | no | inferred / `png` | `png` or `jpeg`/`jpg` |
| `output_key` | string | no | `"grayscale_image"` | Prefix for output values |

## Context Output

- `<output_key>` — output file path
- `<output_key>_width` / `<output_key>_height`
- `<output_key>_format`
- `<output_key>_success`

## Example

```lua
local flow = Flow.new("image_grayscale_demo")

flow:step("grayscale", nodes.image_grayscale({
    path = "data/samples/sample_front.png",
    output_path = "output/sample_front_gray.png",
    output_key = "gray"
}))

flow:step("log", nodes.log({
    message = "Grayscale image: ${ctx.gray_width}x${ctx.gray_height}"
})):depends_on("grayscale")

return flow
```

