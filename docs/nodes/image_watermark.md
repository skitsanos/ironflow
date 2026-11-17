# `image_watermark`

Darken a semi-transparent rectangular band at one position of a single image and write it as PNG or JPEG. The text is not drawn; its length sets the band width.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | One of `path` or `source_key` | — | Source image path; supports `${ctx.key}` interpolation. |
| `source_key` | string | One of `path` or `source_key` | — | Context key holding a path, an artifact (URI, descriptor or `{ artifact = ... }`), or `{ base64 = "..." }`; a stored path is not interpolated. |
| `output_path` | string | Yes | — | Destination file; supports `${ctx.key}` interpolation. |
| `text` | string | No | `"watermark"` | Text that sizes the band and is reported in the output; supports `${ctx.key}` interpolation. |
| `position` | string | No | `"bottom-right"` | `bottom-right`, `bottom-left`, `top-right`, `top-left` or `center`. |
| `opacity` | number | No | `0.5` | Band darkness from `0.0` (none) to `1.0` (black); values outside are clamped. |
| `format` | string | No | from `output_path`, else `png` | Output format: `png`, `jpeg` or `jpg`. |
| `output_key` | string | No | `"image_watermark"` | Prefix for the context output keys. |

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}_path` | string | Output file path. |
| `{output_key}_text` | string | Interpolated `text`. |
| `{output_key}_success` | boolean | `true` on success. |

## Behavior

- The band is 5% of the image height (at least 10 pixels) and about 0.6 band heights wide per text byte, capped at the image width.
- An unknown `position` is treated as `bottom-right`.
- Setting both `path` and `source_key` fails the step.
- Input formats are detected from the content: BMP, Farbfeld, GIF, HDR, ICO, JPEG, PNG, PNM, QOI, TGA, TIFF and WebP.
- Without `format`, a `.png`, `.jpg` or `.jpeg` extension selects the format, no extension writes PNG, and any other extension fails the step.
- The destination directory must exist. A destination that is a symlink or not a regular file fails the step; a failure leaves an existing file unchanged.

## Limits

| Variable | Default | Effect |
|----------|---------|--------|
| `IRONFLOW_MAX_IMAGE_ENCODED_BYTES` | 50 MiB | Maximum encoded source size, including decoded Base64. |
| `IRONFLOW_MAX_IMAGE_PIXELS` | 25,000,000 | Maximum pixels in the source. |
| `IRONFLOW_MAX_IMAGE_DECODE_ALLOCATION_BYTES` | 128 MiB | Maximum decoded source plus RGBA output memory. |

## Examples

### Band in the bottom-right corner

```lua
local flow = Flow.new("image_watermark_demo")

flow:step("watermark", nodes.image_watermark({
    path = "examples/fixtures/ironflow-sample.png",
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
