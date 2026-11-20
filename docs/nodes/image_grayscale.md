# `image_grayscale`

Convert a single image to grayscale and write it as PNG or JPEG.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | One of `path` or `source_key` | — | Source image path; supports `${ctx.key}` interpolation. |
| `source_key` | string | One of `path` or `source_key` | — | Context key holding a path, an artifact (URI, descriptor or `{ artifact = ... }`), or `{ base64 = "..." }`; a stored path is not interpolated. |
| `output_path` | string | Yes | — | Destination file; supports `${ctx.key}` interpolation. |
| `format` | string | No | from `output_path`, else `png` | Output format: `png`, `jpeg` or `jpg`. |
| `output_key` | string | No | `"grayscale_image"` | Prefix for the context output keys. |

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}` | string | Output file path. |
| `{output_key}_width` | number | Output width in pixels. |
| `{output_key}_height` | number | Output height in pixels. |
| `{output_key}_format` | string | `"png"` or `"jpeg"`. |
| `{output_key}_success` | boolean | `true` on success. |

## Behavior

- An alpha channel is kept: a source with transparency becomes grayscale with alpha.
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

### Grayscale a PNG

```lua
local flow = Flow.new("image_grayscale_demo")

flow:step("grayscale", nodes.image_grayscale({
    path = "examples/fixtures/ironflow-sample.png",
    output_path = "output/sample_front_gray.png",
    output_key = "gray"
}))

flow:step("log", nodes.log({
    message = "Grayscale image: ${ctx.gray_width}x${ctx.gray_height}"
})):depends_on("grayscale")

return flow
```
