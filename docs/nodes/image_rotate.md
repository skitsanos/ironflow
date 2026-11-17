# `image_rotate`

Rotate a single image clockwise by 90, 180 or 270 degrees and write it as PNG or JPEG.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | One of `path` or `source_key` | — | Source image path; supports `${ctx.key}` interpolation. |
| `source_key` | string | One of `path` or `source_key` | — | Context key holding a path, an artifact (URI, descriptor or `{ artifact = ... }`), or `{ base64 = "..." }`; a stored path is not interpolated. |
| `output_path` | string | Yes | — | Destination file; supports `${ctx.key}` interpolation. |
| `angle` | number | No | `90` | `90`, `180` or `270`; any other value fails the step. |
| `format` | string | No | from `output_path`, else `png` | Output format: `png`, `jpeg` or `jpg`. |
| `output_key` | string | No | `"rotated_image"` | Prefix for the context output keys. |

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}` | string | Output file path. |
| `{output_key}_angle` | number | Angle applied. |
| `{output_key}_width` | number | Output width in pixels. |
| `{output_key}_height` | number | Output height in pixels. |
| `{output_key}_source_width` | number | Source width in pixels. |
| `{output_key}_source_height` | number | Source height in pixels. |
| `{output_key}_format` | string | `"png"` or `"jpeg"`. |
| `{output_key}_success` | boolean | `true` on success. |

## Behavior

- `angle = 0` is treated as `90`, not as no rotation.
- Setting both `path` and `source_key` fails the step.
- Input formats are detected from the content: BMP, Farbfeld, GIF, HDR, ICO, JPEG, PNG, PNM, QOI, TGA, TIFF and WebP.
- Without `format`, a `.png`, `.jpg` or `.jpeg` extension selects the format, no extension writes PNG, and any other extension fails the step.
- The destination directory must exist. A destination that is a symlink or not a regular file fails the step; a failure leaves an existing file unchanged.

## Limits

| Variable | Default | Effect |
|----------|---------|--------|
| `IRONFLOW_MAX_IMAGE_ENCODED_BYTES` | 50 MiB | Maximum encoded source size, including decoded Base64. |
| `IRONFLOW_MAX_IMAGE_PIXELS` | 25,000,000 | Maximum pixels in the source. |
| `IRONFLOW_MAX_IMAGE_DECODE_ALLOCATION_BYTES` | 128 MiB | Maximum decoded source plus output memory. |

## Examples

### Rotate a quarter turn

```lua
local flow = Flow.new("image_rotate_demo")

flow:step("rotate", nodes.image_rotate({
    path = "examples/fixtures/ironflow-sample.png",
    angle = 90,
    output_path = "output/sample_front_rotated.png",
    output_key = "rotated"
}))

flow:step("log", nodes.log({
    message = "Rotated image: ${ctx.rotated_width}x${ctx.rotated_height}"
})):depends_on("rotate")

return flow
```
