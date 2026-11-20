# `image_to_pdf`

Combine one or more images into a PDF, one image per page.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `sources` | array | One of `sources` or `source_key` | — | Non-empty array of image entries (see below); path strings support `${ctx.key}` interpolation. |
| `source_key` | string | One of `sources` or `source_key` | — | Context key holding an array of image entries; values are used as stored, without interpolation. |
| `output_path` | string | Yes | — | Destination PDF path; supports `${ctx.key}` interpolation. |
| `output_key` | string | No | `"pdf_path"` | Context key for the output path, and prefix for the other keys. |

`sources` and `source_key` are mutually exclusive. Each entry is one of:

| Form | Example |
|------|---------|
| Path or artifact URI string | `"images/logo.png"` |
| Path object | `{ path = "images/logo.png" }` |
| Base64 object | `{ base64 = "iVBORw0KGgo..." }` (`data` is an alias) |
| Artifact descriptor or wrapper | `{ artifact_uri = "artifact://sha256/...", sha256 = "...", size_bytes = 184322, mime_type = "image/png" }` or `{ artifact = ... }` |

An object with more than one of `path`, `base64`, `data`, `artifact` or `artifact_uri` fails the step. The `images` array from [`pdf_to_image`](pdf_to_image.md) can be passed directly with `source_key`.

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}` | string | Output PDF path after interpolation. |
| `image_count` | number | Number of images added. |
| `{output_key}_count` | number | Same as `image_count`. |
| `{output_key}_success` | boolean | `true` after the PDF is committed. |

## Behavior

- Supported formats: BMP, Farbfeld, GIF, HDR, ICO, JPEG, PNG, PNM, QOI, TGA, TIFF and WebP. AVIF and OpenEXR fail the step.
- Each page is the image's pixel size in PDF points. JPEG data is embedded without re-encoding.
- The destination directory must already exist; it is not created.
- A destination that is a symlink or not a regular file fails the step; an existing regular file is replaced.
- Any failure leaves an existing destination unchanged.
- Prefer paths or artifacts over Base64 for large images.

## Limits

| Variable | Default | Effect |
|----------|---------|--------|
| `IRONFLOW_MAX_IMAGE_TO_PDF_SOURCES` | `100` | Maximum number of images. |
| `IRONFLOW_MAX_IMAGE_ENCODED_BYTES` | 50 MiB | Maximum encoded size of each image. |
| `IRONFLOW_MAX_IMAGE_PIXELS` | `25000000` | Maximum pixels of each image. |
| `IRONFLOW_MAX_IMAGE_DECODE_ALLOCATION_BYTES` | 128 MiB | Maximum decoder memory for each image. |
| `IRONFLOW_MAX_IMAGE_TO_PDF_ENCODED_BYTES` | 100 MiB | Maximum total encoded size of all images. |
| `IRONFLOW_MAX_IMAGE_TO_PDF_PIXELS` | `50000000` | Maximum total pixels of all images. |

## Examples

### Combine two images

```lua
local flow = Flow.new("images_to_pdf")

flow:step("make_pdf", nodes.image_to_pdf({
    sources = {
        "data/images/front.png",
        "data/images/back.png",
    },
    output_path = "output/gallery.pdf",
    output_key = "pdf_file"
}))

flow:step("log", nodes.log({
    message = "Wrote ${ctx.pdf_file} with ${ctx.pdf_file_count} page(s)"
})):depends_on("make_pdf")

return flow
```
