# `extract_html`

Extract text and metadata from an HTML file.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | one of `path` or `source_key` | — | File path to the HTML file; supports `${ctx.*}` interpolation. |
| `source_key` | string | one of `path` or `source_key` | — | Context key whose value is the file path (must be a string). |
| `format` | string | no | `"text"` | Output format: `"text"` for sanitized plain text, `"markdown"` for full HTML-to-Markdown conversion. |
| `output_key` | string | no | `"content"` | Context key where the extracted content is stored. |
| `metadata_key` | string | no | — | If set, HTML metadata is stored under this context key. |

> Providing both `path` and `source_key` is an error.
> The `format` parameter only accepts `"text"` or `"markdown"`; any other value is rejected.

## Context Output

- `<output_key>` (default `content`) — the extracted text or Markdown.
- `<metadata_key>` (only when `metadata_key` is set) — an object with available fields: `title`, `description`, `author`, `keywords`, `viewport`, `og:title`, `og:description`, `og:type`, `og:url`.

## Example

```lua
local flow = Flow.new("read_html_file")

flow:step("extract", nodes.extract_html({
    path = "/data/page.html",
    format = "markdown",
    output_key = "html_content",
    metadata_key = "html_meta"
}))

flow:step("done", nodes.log({
    message = "Title: ${ctx.html_meta.title}, Content: ${ctx.html_content}"
})):depends_on("extract")

return flow
```
