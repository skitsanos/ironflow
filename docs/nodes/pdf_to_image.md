# `pdf_to_image`

Render PDF pages to images using the native `pdfium` library at runtime.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | one of `path` or `source_key` | — | File path to the PDF; supports `${ctx.key}` interpolation. |
| `source_key` | string | one of `path` or `source_key` | — | Context key whose value is a file path, artifact URI, or artifact descriptor. |
| `pages` | string | no | `"all"` | Page specification: `"all"`, a single page `"3"`, a range `"1-5"`, or a combination `"1-3,7,9-11"`. Pages are 1-based. |
| `format` | string | no | `"png"` | Image format: `"png"` or `"jpeg"`; `"jpg"` is accepted as an input alias for `"jpeg"`. |
| `dpi` | number | no | `150.0` | Resolution in dots per inch for rendering. |
| `output_key` | string | no | `"images"` | Context key where the array of rendered image objects is stored. |

> Providing both `path` and `source_key` is an error.
> Requires the `pdfium` native library. Set `PDFIUM_LIB_PATH` env var, place `libpdfium` in the working directory, or install it system-wide.

## Context Output

- `<output_key>` (default `images`) — an array of objects, one per rendered page, each containing:
  - `page` — 1-based page number.
  - `width` — rendered image width in pixels.
  - `height` — rendered image height in pixels.
  - `format` — normalized image format (`"png"` or `"jpeg"`).
  - `artifact` — immutable image descriptor with `artifact_uri`, `sha256`, `size_bytes`, and `mime_type`.
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

flow:step("summarize", function(ctx)
    return { rendered_count = #(ctx.images or {}) }
end):depends_on("render")

flow:step("done", nodes.log({
    message = "Rendered ${ctx.rendered_count} of ${ctx.page_count} PDF pages"
})):depends_on("summarize")

return flow
```

## Limits

`pdf_to_image` opens a bounded regular file (or resolved artifact) and lets
Pdfium seek/read the portions it needs instead of copying the complete PDF into
a Rust byte buffer. Each rendered pixel buffer is unavoidable, but its encoded
PNG/JPEG is written directly to the artifact store's private seekable staging
file, hashed from disk, and atomically published; image bytes and Base64 strings
never enter workflow context.
Rendering runs on a tracked blocking worker. The following limits are enforced:

- `IRONFLOW_MAX_PDF_BYTES` — maximum PDF file size, default `104857600`.
- `IRONFLOW_MAX_PDF_RENDER_PAGES` — maximum pages rendered by one node call, default `25`.
- `IRONFLOW_MAX_PDF_RENDER_PIXELS` — maximum pixels per rendered page, default `25000000`.
- `IRONFLOW_MAX_PDF_DPI` — maximum accepted DPI, default `300`.
- `IRONFLOW_MAX_FILE_BYTES` — maximum encoded PNG/JPEG artifact size per page, default `52428800`.

Artifacts are published below `IRONFLOW_ARTIFACT_DIR` (default
`data/artifacts`) and are not automatically expired. Multi-host or recovered
flows require the same artifact directory to be available to every worker. If
a later requested page fails, artifacts already published for earlier pages
remain available but no partial node output is returned; retention must reclaim
unreferenced files.
