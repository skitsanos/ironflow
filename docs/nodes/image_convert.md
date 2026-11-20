# `image_convert`

Re-encode a single image as PNG or JPEG. The output format comes from the `output_path` extension.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | One of `path` or `source_key` | — | Source image path; supports `${ctx.key}` interpolation. |
| `source_key` | string | One of `path` or `source_key` | — | Context key holding a path, an artifact (URI, descriptor or `{ artifact = ... }`), or `{ base64 = "..." }`; a stored path is not interpolated. |
| `output_path` | string | Yes | — | Destination file ending in `.png`, `.jpg` or `.jpeg`; supports `${ctx.key}` interpolation. |
| `quality` | number | No | `85` | JPEG quality from 1 to 100; ignored for PNG. |
| `output_key` | string | No | `"image_convert"` | Prefix for the context output keys. |

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}_path` | string | Output file path. |
| `{output_key}_format` | string | Lowercased `output_path` extension, for example `"jpg"` or `"png"`. |
| `{output_key}_success` | boolean | `true` on success. |

## Behavior

- There is no `format` parameter. An extension other than `.png`, `.jpg` or `.jpeg` fails the step; no extension writes PNG and `{output_key}_format` is then `""`.
- JPEG output drops any alpha channel.
- A `quality` outside 1 to 100, or not a whole number, fails the step, also for PNG output.
- Setting both `path` and `source_key` fails the step.
- Input formats are detected from the content: BMP, Farbfeld, GIF, HDR, ICO, JPEG, PNG, PNM, QOI, TGA, TIFF and WebP.
- The destination directory must exist. A destination that is a symlink or not a regular file fails the step; a failure leaves an existing file unchanged.

## Limits

| Variable | Default | Effect |
|----------|---------|--------|
| `IRONFLOW_MAX_IMAGE_ENCODED_BYTES` | 50 MiB | Maximum encoded source size, including decoded Base64. |
| `IRONFLOW_MAX_IMAGE_PIXELS` | 25,000,000 | Maximum pixels in the source. |
| `IRONFLOW_MAX_IMAGE_DECODE_ALLOCATION_BYTES` | 128 MiB | Maximum decoded source memory, plus the RGB buffer for JPEG output. |

## Examples

### PNG to JPEG

```lua
local flow = Flow.new("image_convert_demo")

flow:step("convert", nodes.image_convert({
    path = "examples/fixtures/ironflow-sample.png",
    output_path = "output/photo.jpg",
    quality = 90
}))

flow:step("log", nodes.log({
    message = "Converted to: ${ctx.image_convert_path} (${ctx.image_convert_format})"
})):depends_on("convert")

return flow
```
