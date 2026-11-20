# `pdf_to_image`

Render PDF pages to PNG or JPEG images stored as artifacts, using the built-in renderer.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | One of `path` or `source_key` | — | PDF file path or `artifact://` URI; supports `${ctx.key}` interpolation. |
| `source_key` | string | One of `path` or `source_key` | — | Context key holding a path, artifact URI or artifact descriptor; used as stored, without interpolation. |
| `pages` | string | No | `"all"` | `"all"`, `"3"`, `"1-5"`, or a list such as `"1-3,7,9-11"`; 1-based. |
| `format` | string | No | `"png"` | `"png"` or `"jpeg"` (alias `"jpg"`). |
| `dpi` | number | No | `150` | Rendering resolution; must be positive and at most `IRONFLOW_MAX_PDF_DPI`. |
| `output_key` | string | No | `"images"` | Context key for the array of rendered images. |

`path` and `source_key` are mutually exclusive.

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}` | array | One image object per rendered page, in selection order. |
| `page_count` | number | Total pages in the PDF, not the number rendered. |

## Output Shape

```json
[
  {
    "page": 1,
    "width": 1275,
    "height": 1650,
    "format": "png",
    "artifact": {
      "artifact_uri": "artifact://sha256/9f2c...",
      "sha256": "9f2c...",
      "size_bytes": 182340,
      "mime_type": "image/png"
    }
  }
]
```

`format` is normalized to `"png"` or `"jpeg"`. Image bytes are stored in the artifact store, never in context.

## Behavior

- The node exists only in builds with the default `pdf-render` Cargo feature.
- A password-protected PDF, a page past the end, or an invalid `pages` value fails the step.
- Embedded fonts, vector graphics, transparency and the common image codecs are rendered without a native library.
- One side of a rendered image is at most 65535 pixels.
- If a later page fails, artifacts already stored for earlier pages remain, but the step returns no output.
- Artifacts are stored under `IRONFLOW_ARTIFACT_DIR` (default `data/artifacts`) and do not expire automatically; see [`docs/CLI_REFERENCE.md`](../CLI_REFERENCE.md).

## Limits

| Variable | Default | Effect |
|----------|---------|--------|
| `IRONFLOW_MAX_PDF_BYTES` | 100 MiB | Maximum PDF size. |
| `IRONFLOW_MAX_PDF_RENDER_PAGES` | `25` | Maximum pages rendered in one step. |
| `IRONFLOW_MAX_PDF_RENDER_PIXELS` | `25000000` | Maximum pixels per rendered page. |
| `IRONFLOW_MAX_PDF_DPI` | `300` | Maximum `dpi`. |
| `IRONFLOW_MAX_PDF_RENDER_OPERATIONS` | `1000000` | Maximum drawing operations per page, counted before drawing. |
| `IRONFLOW_MAX_IMAGE_PIXELS` | `25000000` | Maximum pixels of one image embedded in a page. |
| `IRONFLOW_MAX_PDF_DECOMPRESSED_STREAM_BYTES` | 64 MiB | Maximum decompressed size of one stream; eight times this per step in total. |
| `IRONFLOW_MAX_PDF_OBJECTS` | `250000` | Maximum objects in the PDF. |
| `IRONFLOW_MAX_FILE_BYTES` | 50 MiB | Maximum encoded size of each image. |

See [PDF loading](../PDF_LOADING.md) for how the stream and object limits apply.

## Examples

### Render a page range

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
