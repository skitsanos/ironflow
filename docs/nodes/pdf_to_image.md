# `pdf_to_image`

Render PDF pages to images using the native `pdfium` library at runtime.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | one of `path` or `source_key` | — | File path to the PDF; supports `${ctx.*}` interpolation. |
| `source_key` | string | one of `path` or `source_key` | — | Context key whose value is the file path (must be a string). |
| `pages` | string | no | `"all"` | Page specification: `"all"`, a single page `"3"`, a range `"1-5"`, or a combination `"1-3,7,9-11"`. Pages are 1-based. |
| `format` | string | no | `"png"` | Image format: `"png"`, `"jpeg"`, or `"jpg"`. |
| `dpi` | number | no | `150.0` | Resolution in dots per inch for rendering. |
| `output_key` | string | no | `"images"` | Context key where the array of rendered image objects is stored. |

> Providing both `path` and `source_key` is an error.
> Requires the `pdfium` native library. Set `PDFIUM_LIB_PATH` env var, place `libpdfium` in the working directory, or install it system-wide.

## Context Output

- `<output_key>` (default `images`) — an array of objects, one per rendered page, each containing:
  - `page` — 1-based page number.
  - `width` — rendered image width in pixels.
  - `height` — rendered image height in pixels.
  - `format` — the image format (`"png"`, `"jpeg"`, or `"jpg"`).
  - `image_base64` — base64-encoded image data.
- `page_count` — total number of pages in the PDF document.

## Example

```lua
local flow = Flow.new("render_pdf_pages")

flow:step("render", nodes.pdf_to_image({
    path = "/data/document.pdf",
    pages = "1-3",
    format = "png",
    dpi = 200,
    output_key = "images"
}))

flow:step("done", nodes.log({
    message = "Rendered ${ctx.page_count} total pages, got ${ctx.images} image(s)"
})):depends_on("render")

return flow
```
