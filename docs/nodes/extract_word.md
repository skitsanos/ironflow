# `extract_word`

Extract text and metadata from a Word (.docx) document.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | one of `path` or `source_key` | — | File path to the `.docx` file; supports `${ctx.*}` interpolation. |
| `source_key` | string | one of `path` or `source_key` | — | Context key whose value is the file path (must be a string). |
| `format` | string | no | `"text"` | Output format: `"text"` for plain text, `"markdown"` for Markdown with headings, lists, and inline formatting. |
| `output_key` | string | no | `"content"` | Context key where the extracted text/markdown is stored. |
| `metadata_key` | string | no | — | If set, document metadata (Dublin Core fields) is stored under this context key. |

> Providing both `path` and `source_key` is an error.
> The `format` parameter only accepts `"text"` or `"markdown"`; any other value is rejected.

## Context Output

- `<output_key>` (default `content`) — the extracted document text or Markdown.
- `<metadata_key>` (only when `metadata_key` is set) — an object with available fields: `title`, `author`, `subject`, `description`, `keywords`, `last_modified_by`, `created`, `modified`, `revision`, `category`.

## Example

```lua
local flow = Flow.new("read_word_doc")

flow:step("extract", nodes.extract_word({
    path = "/data/report.docx",
    format = "markdown",
    output_key = "doc_content",
    metadata_key = "doc_meta"
}))

flow:step("done", nodes.log({
    message = "Author: ${ctx.doc_meta.author}, Content: ${ctx.doc_content}"
})):depends_on("extract")

return flow
```
