# `extract_pdf`

Extract text and metadata from a PDF document.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | one of `path` or `source_key` | — | File path to the PDF; supports `${ctx.*}` interpolation. |
| `source_key` | string | one of `path` or `source_key` | — | Context key whose value is the file path (must be a string). |
| `format` | string | no | `"text"` | Output format: `"text"` for raw extracted text, `"markdown"` for best-effort paragraph-grouped Markdown. |
| `output_key` | string | no | `"content"` | Context key where the extracted text is stored. |
| `metadata_key` | string | no | — | If set, PDF metadata is stored under this context key. |

> Providing both `path` and `source_key` is an error.
> The `format` parameter only accepts `"text"` or `"markdown"`; any other value is rejected.

## Context Output

- `<output_key>` (default `content`) — the extracted text or Markdown.
- `<metadata_key>` (only when `metadata_key` is set) — an object with available fields: `pages` (number), `title`, `author`, `subject`, `keywords`, `creator`, `producer`, `created`, `modified`.

## Example

```lua
local flow = Flow.new("read_pdf")

flow:step("extract", nodes.extract_pdf({
    path = "${ctx.file_path}",
    format = "text",
    output_key = "pdf_text",
    metadata_key = "pdf_meta"
}))

flow:step("done", nodes.log({
    message = "Pages: ${ctx.pdf_meta.pages}, Content: ${ctx.pdf_text}"
})):depends_on("extract")

return flow
```
