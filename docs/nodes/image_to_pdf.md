# `image_to_pdf`

Convert one or more images into a single PDF file.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `sources` | array | one of `sources` or `source_key` | — | Array of source images. Each item can be a file path string or object: `{ path = "..." }` or `{ base64 = "..." }`. |
| `source_key` | string | one of `sources` or `source_key` | — | Context key containing an array of source images (same formats as `sources`). |
| `output_path` | string | yes | — | Destination path for generated PDF. Supports `${ctx.*}` interpolation. |
| `output_key` | string | no | `pdf_path` | Context key to store generated PDF path. |

Paths inside image entries support `${ctx.*}` interpolation.

### Image entry formats

#### String

```json
"images/logo.png"
```

#### Object

```lua
{ path = "images/logo.png" }
```

```lua
{ base64 = "iVBORw0KGgoAAAANSUhEUg..." }
```

## Context Output

- `<output_key>` (default `pdf_path`) — output PDF path.
- `image_count` — number of source images processed.
- `<output_key>_count` — same as `image_count`.
- `<output_key>_success` — boolean `true`.

## Example

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
