# `pdf_metadata`

Extract metadata from a PDF file (document info dictionary + page count).

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | one of `path` or `source_key` | — | PDF file path |
| `source_key` | string | one of `path` or `source_key` | — | Context key containing a file path |
| `output_key` | string | no | `"metadata"` | Prefix for output key |

> Providing both `path` and `source_key` is an error.

## Context Output

- `<output_key>` — object containing metadata:
  - `pages` — page count
  - `title`, `author`, `subject`, `keywords`, `creator`, `producer`, `created`, `modified` when present

## Example

```lua
local flow = Flow.new("pdf_metadata_demo")

flow:step("meta", nodes.pdf_metadata({
    path = "data/samples/Bill26022026_121916AM_8000951511_fc72420d-72e1-460b-b714-8a7388ea90d4_.pdf",
    output_key = "pdf_meta"
}))

flow:step("log", nodes.log({
    message = "PDF has ${ctx.pdf_meta.pages} page(s), produced by ${ctx.pdf_meta.creator or 'unknown'}"
})):depends_on("meta")

return flow
```

