# `pdf_to_image`

Render PDF pages to images with the built-in renderer; no native library is needed.
The node exists only in builds with the default `pdf-render` Cargo feature;
`--no-default-features` drops it together with the rasterizer.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | one of `path` or `source_key` | — | File path to the PDF; supports `${ctx.key}` interpolation. |
| `source_key` | string | one of `path` or `source_key` | — | Context key whose value is a file path, artifact URI, or artifact descriptor. A stored path is used as it is and is not interpolated. |
| `pages` | string | no | `"all"` | Page specification: `"all"`, a single page `"3"`, a range `"1-5"`, or a combination `"1-3,7,9-11"`. Pages are 1-based. |
| `format` | string | no | `"png"` | Image format: `"png"` or `"jpeg"`; `"jpg"` is accepted as an input alias for `"jpeg"`. |
| `dpi` | number | no | `150.0` | Resolution in dots per inch for rendering. |
| `output_key` | string | no | `"images"` | Context key where the array of rendered image objects is stored. |

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

`pdf_to_image` reads a bounded regular file (or resolved artifact) into memory
once, applies the shared loading policy, and admits each page before drawing
it. Each rendered pixel buffer (four bytes per pixel) is unavoidable, but its
encoded PNG/JPEG is written directly to the artifact store's private seekable
staging file, hashed from disk, and atomically published; image bytes and
Base64 strings never enter workflow context.
Rendering runs on a tracked blocking worker. Cancellation, a step timeout and
the run deadline are observed while a page is being admitted (every 256
drawing operations) and between pages; drawing an admitted page is not
interrupted, and is bounded by the operation and pixel limits. The following
limits are enforced:

- `IRONFLOW_MAX_PDF_BYTES` — maximum PDF file size, default `104857600`.
- `IRONFLOW_MAX_PDF_RENDER_PAGES` — maximum pages rendered by one node call, default `25`.
- `IRONFLOW_MAX_PDF_RENDER_PIXELS` — maximum pixels per rendered page, default `25000000`.
- `IRONFLOW_MAX_PDF_DPI` — maximum accepted DPI, default `300`.
- `IRONFLOW_MAX_PDF_RENDER_OPERATIONS` — maximum drawing operations per page, default `1000000`. Each path, clip, transparency group, glyph and image counts as one, an image adds one per 1024 pixels, and a tiling pattern counts its content once per tile copy. Counted before the page is drawn, so a page with exponentially nested forms fails instead of running.
- `IRONFLOW_MAX_IMAGE_PIXELS` — maximum pixels of one image embedded in a rendered page, default `25000000`.
- `IRONFLOW_MAX_PDF_DECOMPRESSED_STREAM_BYTES` and `IRONFLOW_MAX_PDF_OBJECTS` — the shared [PDF loading limits](../PDF_LOADING.md). Streams reachable from a rendered page are additionally decoded under the per-stream limit, with eight times that limit as the total for one node call.
- `IRONFLOW_MAX_FILE_BYTES` — maximum encoded PNG/JPEG artifact size per page, default `52428800`.

Artifacts are published below `IRONFLOW_ARTIFACT_DIR` (default
`data/artifacts`) and are not automatically expired. Multi-host or recovered
flows require the same artifact directory to be available to every worker. If
a later requested page fails, artifacts already published for earlier pages
remain available but no partial node output is returned; retention must reclaim
unreferenced files.
