# `image_crop`

Crop a rectangle from a single image and write it as PNG or JPEG.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | One of `path` or `source_key` | — | Source image path; supports `${ctx.key}` interpolation. |
| `source_key` | string | One of `path` or `source_key` | — | Context key holding a path, an artifact (URI, descriptor or `{ artifact = ... }`), or `{ base64 = "..." }`; a stored path is not interpolated. |
| `output_path` | string | Yes | — | Destination file; supports `${ctx.key}` interpolation. |
| `x` | number | No | `0` | Left offset in pixels. |
| `y` | number | No | `0` | Top offset in pixels. |
| `crop_width` | number | Yes (or `width`) | — | Crop width in pixels (at least 1); `width` is an alias. |
| `crop_height` | number | Yes (or `height`) | — | Crop height in pixels (at least 1); `height` is an alias. |
| `format` | string | No | from `output_path`, else `png` | Output format: `png`, `jpeg` or `jpg`. |
| `output_key` | string | No | `"cropped_image"` | Prefix for the context output keys. |

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}` | string | Output file path. |
| `{output_key}_width` | number | Crop width. |
| `{output_key}_height` | number | Crop height. |
| `{output_key}_x` | number | Left offset used. |
| `{output_key}_y` | number | Top offset used. |
| `{output_key}_format` | string | `"png"` or `"jpeg"`. |
| `{output_key}_success` | boolean | `true` on success. |

## Behavior

- The rectangle must fit inside the image: a start point outside it, or a width or height that runs past its edge, fails the step. The crop is not clipped.
- `crop_width` and `crop_height` win over `width` and `height` when both are set.
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

### Crop a region

```lua
local flow = Flow.new("image_crop_demo")

flow:step("crop", nodes.image_crop({
    path = "examples/fixtures/ironflow-sample.png",
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
