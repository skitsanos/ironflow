# `pdf_thumbnail`

Render a single PDF page to an image with the built-in renderer; no native library is needed.
The node exists only in builds with the default `pdf-render` Cargo feature;
`--no-default-features` drops it together with the rasterizer.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | one of `path` or `source_key` | — | File path to the PDF; supports `${ctx.key}` interpolation. |
| `source_key` | string | one of `path` or `source_key` | — | Context key whose value is a file path, artifact URI, or artifact descriptor. A stored path is used as it is and is not interpolated. |
| `page` | number | no | `1` | 1-based page number to render. |
| `format` | string | no | `png` | Image format: `png` or `jpeg`; `jpg` is accepted as an input alias for `jpeg`. |
| `width` | number | no | — | Exact thumbnail width in pixels. If set without `height`, height is auto-scaled. |
| `height` | number | no | — | Exact thumbnail height in pixels. If set without `width`, width is auto-scaled. |
| `size` | number | no | `256` | Maximum side length when neither `width` nor `height` is provided. |
| `dpi` | number | no | `150` | Resolution in dots per inch for rendering before scaling. |
| `output_key` | string | no | `"thumbnail"` | Context key to store thumbnail object. |

> Providing both `path` and `source_key` is an error.
> Artifact inputs are opened and SHA-256 verified inside the tracked blocking worker; the renderer reads that same rewound handle rather than a resolved store pathname.

## Renderer

Pages are drawn by a built-in pure-Rust rasterizer ([hayro](https://crates.io/crates/hayro)
0.9), so the node works in every default build and in the container image with
no native library. It renders embedded TrueType, OpenType, CFF, Type 1 and
Type 3 fonts, the standard 14 fonts, vector graphics, shadings, tiling
patterns, soft masks, blend modes, and Flate, LZW, DCT (JPEG), JPEG 2000,
JBIG2 and CCITT images, including the multilingual output of
[`markdown_to_pdf`](markdown_to_pdf.md). Known gaps:

- Password-protected input is refused with `PDF requires a password`.
- Text in a font that is not embedded is drawn with the closest of the
  standard 14 fonts; a CJK font that is not embedded has no substitute.
- Knockout groups and non-isolated transparency groups are composited as
  ordinary groups, and color-key image masking is ignored, so such pages can
  differ from a reference viewer.
- Interactive form fields and annotations are drawn only from the appearance
  streams stored in the file; none is generated.
- A stream or image the rasterizer cannot decode is left out of the page; the
  page still renders.
- One side of the rendered image is at most 65535 pixels.

Pdfium is no longer used or supported: `PDFIUM_LIB_PATH` is ignored.

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
- `IRONFLOW_MAX_PDF_RENDER_OPERATIONS` — maximum drawing operations per page, default `1000000`. Each path, clip, transparency group, glyph and image counts as one, an image adds one per 1024 pixels, and a tiling pattern counts its content once per tile copy. Counted before the page is drawn, so a page with exponentially nested forms fails instead of running.
- `IRONFLOW_MAX_IMAGE_PIXELS` — maximum pixels of one image embedded in a rendered page, default `25000000`.
- `IRONFLOW_MAX_PDF_DECOMPRESSED_STREAM_BYTES` and `IRONFLOW_MAX_PDF_OBJECTS` — the shared [PDF loading limits](../PDF_LOADING.md). Streams reachable from a rendered page are additionally decoded under the per-stream limit, with eight times that limit as the total for one node call.
- `IRONFLOW_MAX_FILE_BYTES` — maximum encoded thumbnail artifact size, default `52428800`.

The PDF is read once as a bounded file and each page is admitted before it is
drawn, with the cancellation behavior described for
[`pdf_to_image`](pdf_to_image.md#limits). The rendered pixel buffer is encoded directly into the artifact
store's private seekable staging file, hashed from disk, and atomically
published below `IRONFLOW_ARTIFACT_DIR`; no image Base64 is retained in context.
The local artifact store has no automatic expiration and must be shared across
workers when runs can recover on another host.
