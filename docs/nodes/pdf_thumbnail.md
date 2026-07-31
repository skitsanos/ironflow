# `pdf_thumbnail`

Render a single PDF page to an image using the native `pdfium` library at runtime.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | one of `path` or `source_key` | — | File path to the PDF; supports `${ctx.key}` interpolation. |
| `source_key` | string | one of `path` or `source_key` | — | Context key whose value is a file path, artifact URI, or artifact descriptor. |
| `page` | number | no | `1` | 1-based page number to render. |
| `format` | string | no | `png` | Image format: `png` or `jpeg`; `jpg` is accepted as an input alias for `jpeg`. |
| `width` | number | no | — | Exact thumbnail width in pixels. If set without `height`, height is auto-scaled. |
| `height` | number | no | — | Exact thumbnail height in pixels. If set without `width`, width is auto-scaled. |
| `size` | number | no | `256` | Maximum side length when neither `width` nor `height` is provided. |
| `dpi` | number | no | `150` | Resolution in dots per inch for rendering before scaling. |
| `output_key` | string | no | `"thumbnail"` | Context key to store thumbnail object. |

> Providing both `path` and `source_key` is an error.
> Requires the `pdfium` native library. Set `PDFIUM_LIB_PATH`, place `libpdfium` in the working directory, or install system-wide.

## Context Output

- `<output_key>` (default `thumbnail`) — an object containing:
  - `page` — 1-based page number.
  - `width` — rendered thumbnail width in pixels.
  - `height` — rendered thumbnail height in pixels.
  - `format` — normalized image format (`"png"` or `"jpeg"`).
  - `artifact` — immutable image descriptor with `artifact_uri`, `sha256`, `size_bytes`, and `mime_type`.
- `<output_key>_count` — always `1`.

## Example

```lua
local flow = Flow.new("pdf_thumbnail_demo")

flow:step("thumb", nodes.pdf_thumbnail({
    path = "examples/fixtures/ironflow-sample.pdf",
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

## Limits

`pdf_thumbnail` shares the PDF rendering safeguards used by `pdf_to_image`:

- `IRONFLOW_MAX_PDF_BYTES` — maximum PDF file size, default `104857600`.
- `IRONFLOW_MAX_PDF_RENDER_PIXELS` — maximum pixels in the rendered thumbnail, default `25000000`.
- `IRONFLOW_MAX_PDF_DPI` — maximum accepted DPI, default `300`.
- `IRONFLOW_MAX_FILE_BYTES` — maximum encoded thumbnail artifact size, default `52428800`.

The PDF is opened as a bounded seekable file rather than copied into a full
byte vector. The rendered pixel buffer is encoded directly into the artifact
store's private seekable staging file, hashed from disk, and atomically
published below `IRONFLOW_ARTIFACT_DIR`; no image Base64 is retained in context.
The local artifact store has no automatic expiration and must be shared across
workers when runs can recover on another host.
