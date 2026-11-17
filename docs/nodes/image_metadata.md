# `image_metadata`

Read the dimensions, format and color type of a single image from its header, without decoding the pixels.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | One of `path` or `source_key` | — | Source image path; supports `${ctx.key}` interpolation. |
| `source_key` | string | One of `path` or `source_key` | — | Context key holding a path, an artifact (URI, descriptor or `{ artifact = ... }`), or `{ base64 = "..." }`; a stored path is not interpolated. |
| `output_key` | string | No | `"image_metadata"` | Prefix for the context output keys. |

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}_width` | number | Width in pixels. |
| `{output_key}_height` | number | Height in pixels. |
| `{output_key}_format` | string | Format detected from the content, lowercase (for example `png`, `jpeg`, `webp`). |
| `{output_key}_color_type` | string | Color type, for example `Rgb8` or `Rgba8`. |

## Behavior

- The format is detected from the content, so a file needs no extension. Supported: BMP, Farbfeld, GIF, HDR, ICO, JPEG, PNG, PNM, QOI, TGA, TIFF and WebP; anything else fails the step.
- There is no `{output_key}_success` key.
- Setting both `path` and `source_key` fails the step.

## Limits

| Variable | Default | Effect |
|----------|---------|--------|
| `IRONFLOW_MAX_IMAGE_ENCODED_BYTES` | 50 MiB | Maximum encoded source size, including decoded Base64. |
| `IRONFLOW_MAX_IMAGE_PIXELS` | 25,000,000 | Maximum pixels reported by the header. |
| `IRONFLOW_MAX_IMAGE_DECODE_ALLOCATION_BYTES` | 128 MiB | Maximum decoded size reported by the header. |

## Examples

### Read dimensions

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
