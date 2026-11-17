# `extract_html`

Extract the text, and optionally the page metadata, of an HTML file as plain text or Markdown.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | One of `path` or `source_key` | — | Path to the HTML file; supports `${ctx.key}` interpolation. |
| `source_key` | string | One of `path` or `source_key` | — | Context key holding a file path, artifact URI, or artifact descriptor; a stored path is used as is, not interpolated. |
| `format` | string | No | `"text"` | `"text"` for plain text, `"markdown"` for Markdown with headings, emphasis, lists, tables, links and code. |
| `output_key` | string | No | `"content"` | Context key for the extracted content. |
| `metadata_key` | string | No | — | When set, page metadata is stored under this key; must differ from `output_key`. |

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}` | string | Extracted text or Markdown. |
| `{metadata_key}` | object | Only when `metadata_key` is set: whichever of `title`, `description`, `author`, `keywords`, `viewport`, `og:title`, `og:description`, `og:type`, `og:url` the page provides. |

## Behavior

- Setting both `path` and `source_key`, neither, a `format` other than `text`/`markdown`, or a non-string value for a string parameter fails the step. The file must be valid UTF-8.
- Both formats drop `head`, `title`, `script`, `style`, `noscript` and `template` content and comments. Metadata is read from the original input, so the title is still available under `metadata_key`.
- `text` adds no Markdown markers or escaping. Paragraph-level blocks are separated by one blank line; list items, table rows, `dt`/`dd` and `div` blocks by one newline; `<br>` is one newline and never widens a blank line.
- In `text`, table cells are separated by spaces, image alt text is kept, whitespace is collapsed except in preformatted text, and leading and trailing newlines are trimmed.
- `markdown` escapes literal Markdown punctuation only where it would change meaning: `<p># literal</p>` becomes `\# literal`, while `C#` stays unchanged.
- `markdown` fails when elements nest deeper than 128 levels (fixed; the parser's `<html>` and `<body>` count, so a fragment has 126). `text` accepts up to `IRONFLOW_MAX_HTML_TEXT_DEPTH` levels.
- CSS is not evaluated, scripts are not run, and external resources are not fetched. This is extraction, not sanitizing; use [`html_sanitize`](html_sanitize.md) for that.

## Limits

| Variable | Default | Effect |
|----------|---------|--------|
| `IRONFLOW_MAX_FILE_BYTES` | `52428800` (50 MiB) | Maximum input file size. |
| `IRONFLOW_MAX_EXTRACT_ITEMS` | `250000` | Maximum markup items (`<` markers) in the input. |
| `IRONFLOW_MAX_HTML_TEXT_DEPTH` | `1024` | Maximum element nesting for `format = "text"`. |
| `IRONFLOW_MAX_EXTRACT_OUTPUT_BYTES` | `52428800` (50 MiB) | Maximum serialized output, including metadata; Markdown is charged while it is generated. |

Exceeding a limit fails the step. See [`docs/SECURITY.md`](../SECURITY.md) for how inputs are admitted.

## Examples

### Markdown and metadata

See [extract_html.lua](../../examples/08-extraction/extract_html.lua) for a self-contained version that writes its own input.

```lua
local flow = Flow.new("read_html_file")

flow:step("extract", nodes.extract_html({
    path = "/data/page.html",
    format = "markdown",
    output_key = "html_content",
    metadata_key = "html_meta"
}))

flow:step("done", nodes.log({
    message = "Extracted HTML title: ${ctx.html_meta.title}"
})):depends_on("extract")

return flow
```
