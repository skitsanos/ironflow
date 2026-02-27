# `pdf_thumbnail`

Render a single PDF page to an image using the native `pdfium` library at runtime.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | one of `path` or `source_key` | — | File path to the PDF; supports `${ctx.*}` interpolation. |
| `source_key` | string | one of `path` or `source_key` | — | Context key whose value is the file path (must be a string). |
| `page` | number | no | `1` | 1-based page number to render. |
| `format` | string | no | `png` | Image format: `png`, `jpeg`, or `jpg`. |
| `width` | number | no | — | Exact thumbnail width in pixels. If set, height is auto-scaled. |
| `height` | number | no | — | Exact thumbnail height in pixels. If set, width is auto-scaled. |
| `size` | number | no | `256` | Maximum side length when `width` and `height` are not both provided. |
| `dpi` | number | no | `150` | Resolution in dots per inch for rendering before scaling. |
| `output_key` | string | no | `"thumbnail"` | Context key to store thumbnail object. |

> Providing both `path` and `source_key` is an error.
> Requires the `pdfium` native library. Set `PDFIUM_LIB_PATH`, place `libpdfium` in the working directory, or install system-wide.

## Context Output

- `<output_key>` (default `thumbnail`) — an object containing:
  - `page` — 1-based page number.
  - `width` — rendered thumbnail width in pixels.
  - `height` — rendered thumbnail height in pixels.
  - `format` — the image format (`"png"`, `"jpeg"`, or `"jpg"`).
  - `image_base64` — base64-encoded image bytes.
- `<output_key>_count` — always `1`.

## Example

```lua
local flow = Flow.new("pdf_thumbnail_demo")

flow:step("thumb", nodes.pdf_thumbnail({
    path = "data/samples/Bill26022026_121916AM_8000951511_fc72420d-72e1-460b-b714-8a7388ea90d4_.pdf",
    page = 1,
    format = "png",
    size = 320,
    dpi = 150,
    output_key = "preview"
}))

flow:step("show", nodes.log({
    message = "Generated preview ${ctx.preview.width}x${ctx.preview.height}"
})):depends_on("thumb")

return flow
```
