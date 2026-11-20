# `image_resize`

Resize a single image and write it as PNG or JPEG.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | One of `path` or `source_key` | — | Source image path; supports `${ctx.key}` interpolation. |
| `source_key` | string | One of `path` or `source_key` | — | Context key holding a path, an artifact (URI, descriptor or `{ artifact = ... }`), or `{ base64 = "..." }`; a stored path is not interpolated. |
| `output_path` | string | Yes | — | Destination file; supports `${ctx.key}` interpolation. |
| `width` | number | One of `width` or `height` | — | Target width in pixels (at least 1). |
| `height` | number | One of `width` or `height` | — | Target height in pixels (at least 1). |
| `format` | string | No | from `output_path`, else `png` | Output format: `png`, `jpeg` or `jpg`. |
| `output_key` | string | No | `"resized_image"` | Prefix for the context output keys. |

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}` | string | Output file path. |
| `{output_key}_width` | number | Output width in pixels. |
| `{output_key}_height` | number | Output height in pixels. |
| `{output_key}_format` | string | `"png"` or `"jpeg"`. |
| `{output_key}_success` | boolean | `true` on success. |

## Behavior

- With one dimension, the other keeps the aspect ratio (rounded, minimum 1). With both, the image is resized to exactly that size, which may change its aspect ratio. Resampling uses Lanczos3.
- Setting both `path` and `source_key` fails the step.
- Input formats are detected from the content: BMP, Farbfeld, GIF, HDR, ICO, JPEG, PNG, PNM, QOI, TGA, TIFF and WebP.
- Without `format`, a `.png`, `.jpg` or `.jpeg` extension selects the format, no extension writes PNG, and any other extension fails the step.
- The destination directory must exist. A destination that is a symlink or not a regular file fails the step; a failure leaves an existing file unchanged. See [`write_file`](write_file.md#behavior) for file modes.
- The size check covers the source, the output and the resampling buffers, so a large upscale can fail even when the source is small.

## Limits

| Variable | Default | Effect |
|----------|---------|--------|
| `IRONFLOW_MAX_IMAGE_ENCODED_BYTES` | 50 MiB | Maximum encoded source size, including decoded Base64. |
| `IRONFLOW_MAX_IMAGE_PIXELS` | 25,000,000 | Maximum pixels in the source and the output. |
| `IRONFLOW_MAX_IMAGE_DECODE_ALLOCATION_BYTES` | 128 MiB | Maximum estimated working memory for decoding and resizing. |

## Examples

### Resize to a width

```lua
local flow = Flow.new("image_resize_demo")

flow:step("resize", nodes.image_resize({
    path = "examples/fixtures/ironflow-sample.png",
    output_path = "outputs/sample_front_small.png",
    width = 120
}))

flow:step("log", nodes.log({
    message = "Resized to ${ctx.resized_image_width}x${ctx.resized_image_height}"
})):depends_on("resize")

return flow
```
