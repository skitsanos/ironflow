# `image_to_pdf`

Convert one or more images into a single PDF file.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `sources` | array | one of `sources` or `source_key` | — | Array of source images. Each item may be a path, artifact URI/descriptor, `{ artifact = ... }`, or an explicit `{ base64 = "..." }` object. |
| `source_key` | string | one of `sources` or `source_key` | — | Context key containing an array of source images (same formats as `sources`). |
| `output_path` | string | yes | — | Destination path for generated PDF. Supports `${ctx.key}` interpolation. |
| `output_key` | string | no | `pdf_path` | Context key to store generated PDF path. |

Paths inside image entries support `${ctx.key}` interpolation.

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

```lua
{ artifact_uri = "artifact://sha256/0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
  sha256 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
  size_bytes = 184322,
  mime_type = "image/png" }
```

Output wrappers from `pdf_to_image` are also accepted directly: set
`source_key = "images"` and each `{ artifact = ... }` entry in that context
array is resolved without Base64. For one `pdf_thumbnail` result, place the
thumbnail object in a context array before passing its key.

Artifact inputs are opened and SHA-256 verified inside the tracked blocking
worker; encoding reads that same rewound handle rather than a resolved store
pathname.

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

## Resource contract

Source objects must select exactly one of `path`, `artifact`, `base64`, or
`data`; ambiguous objects fail instead of silently selecting the first field.
The node rejects more than `IRONFLOW_MAX_IMAGE_TO_PDF_SOURCES` entries (default
`100`) before parsing them. Each image is bounded by
`IRONFLOW_MAX_IMAGE_ENCODED_BYTES` (50 MiB), `IRONFLOW_MAX_IMAGE_PIXELS` (25
million), and `IRONFLOW_MAX_IMAGE_DECODE_ALLOCATION_BYTES` (128 MiB). A whole
call is additionally bounded by `IRONFLOW_MAX_IMAGE_TO_PDF_ENCODED_BYTES` (100
MiB) and `IRONFLOW_MAX_IMAGE_TO_PDF_PIXELS` (50 million).

Headers, dimensions, decoded byte estimates, and cumulative budgets are checked
before pixel decoding or adding a PDF object. Base64 length is admitted against
the per-source and cumulative decoded-byte ceilings before IronFlow clones a
source string out of workflow context; malformed or oversized later entries do
not cause an unbounded pre-worker copy. JPEG bytes can be embedded without
decoding. Other formats use the configured image decoder limits and a bounded
pixel conversion and compression-buffer preflight. The in-progress `lopdf`
document still retains embedded streams until save, and accepted Base64 input
temporarily retains both its bounded context string and decoded bytes; prefer
paths or artifacts for large media. Decode, conversion, compression, and save
run on a tracked blocking worker with cooperative checkpoints between opaque
codec operations.
