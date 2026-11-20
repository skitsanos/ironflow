# `pdf_thumbnail`

Render one PDF page to a PNG or JPEG thumbnail stored as an artifact, using the built-in renderer.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | One of `path` or `source_key` | — | PDF file path or `artifact://` URI; supports `${ctx.key}` interpolation. |
| `source_key` | string | One of `path` or `source_key` | — | Context key holding a path, artifact URI or artifact descriptor; used as stored, without interpolation. |
| `page` | integer | No | `1` | 1-based page to render. |
| `format` | string | No | `"png"` | `"png"` or `"jpeg"` (alias `"jpg"`). |
| `width` | integer | No | — | Output width in pixels; alone, height follows the page aspect ratio. |
| `height` | integer | No | — | Output height in pixels; alone, width follows the page aspect ratio. |
| `size` | integer | No | `256` | Longest side in pixels when neither `width` nor `height` is set. |
| `dpi` | number | No | `150` | Must be positive and at most `IRONFLOW_MAX_PDF_DPI`; it does not change the output size. |
| `output_key` | string | No | `"thumbnail"` | Context key for the thumbnail object. |

`path` and `source_key` are mutually exclusive. With both `width` and `height` the image is stretched to exactly that size.

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}` | object | The rendered thumbnail. |
| `{output_key}_count` | number | Always `1`. |

## Output Shape

```json
{
  "page": 1,
  "width": 320,
  "height": 414,
  "format": "png",
  "artifact": {
    "artifact_uri": "artifact://sha256/4b1e...",
    "sha256": "4b1e...",
    "size_bytes": 24811,
    "mime_type": "image/png"
  }
}
```

## Behavior

- The node exists only in builds with the default `pdf-render` Cargo feature.
- `page = 0`, a page past the end, a zero size, or a password-protected PDF fails the step.
- Rendering is the same as [`pdf_to_image`](pdf_to_image.md#behavior); one side is at most 65535 pixels.
- The artifact is stored under `IRONFLOW_ARTIFACT_DIR` (default `data/artifacts`) and does not expire automatically.

## Limits

| Variable | Default | Effect |
|----------|---------|--------|
| `IRONFLOW_MAX_PDF_BYTES` | 100 MiB | Maximum PDF size. |
| `IRONFLOW_MAX_PDF_RENDER_PIXELS` | `25000000` | Maximum pixels in the thumbnail. |
| `IRONFLOW_MAX_PDF_DPI` | `300` | Maximum `dpi`. |
| `IRONFLOW_MAX_PDF_RENDER_OPERATIONS` | `1000000` | Maximum drawing operations for the page. |
| `IRONFLOW_MAX_IMAGE_PIXELS` | `25000000` | Maximum pixels of one image embedded in the page. |
| `IRONFLOW_MAX_PDF_DECOMPRESSED_STREAM_BYTES` | 64 MiB | Maximum decompressed size of one stream. |
| `IRONFLOW_MAX_PDF_OBJECTS` | `250000` | Maximum objects in the PDF. |
| `IRONFLOW_MAX_FILE_BYTES` | 50 MiB | Maximum encoded thumbnail size. |

## Examples

### Render a cover preview

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
