# `html_to_markdown`

Convert an HTML string to Markdown. Conversion is best effort and lossy on complex HTML.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `input` | string | One of `input` or `source_key` | — | Literal HTML; supports `${ctx.key}` interpolation. |
| `source_key` | string | One of `input` or `source_key` | — | Context key holding the HTML; a non-string value is serialized as JSON first. |
| `output_key` | string | No | `"markdown"` | Context key for the Markdown. |

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}` | string | Generated Markdown. |

## Behavior

- Setting both `input` and `source_key`, or neither, fails the step; so does a `source_key` that is not in the context.
- Fragments and malformed markup are accepted. `head`, `title`, `script`, `style`, `noscript` and `template` content and comments are dropped, as in [`extract_html`](extract_html.md).
- Headings, emphasis, links, images, inline and fenced code, block quotes, lists and GFM pipe tables are kept.
- Literal Markdown punctuation in prose is escaped (`# literal` becomes `\# literal`); code content is left untouched. `&`, `$` and `~` are always escaped, and `&` also in link destinations (`/r?a=1\&b=2`), so read URLs with a Markdown parser, not from the raw text.
- Link text is one line: block content and `<br>` inside an anchor are joined with spaces (`<a href="/c"><h3>Card</h3><p>Body</p></a>` becomes `[Card Body](/c)`).
- A table with a header cell or `<thead>` becomes a pipe table; a `|` in a cell is written `\|`, and a `<th>` in a body row is dropped. Other tables are written as plain blocks.
- Elements may nest at most 128 levels (fixed; the parser's `<html>` and `<body>` count, so a fragment has 126). Deeper input fails the step.
- CSS and scripts are not evaluated and external resources are not fetched. This is not a sanitizer; use [`html_sanitize`](html_sanitize.md).

## Limits

| Variable | Default | Effect |
|----------|---------|--------|
| `IRONFLOW_MAX_FILE_BYTES` | `52428800` (50 MiB) | Maximum HTML input size. |
| `IRONFLOW_MAX_EXTRACT_ITEMS` | `250000` | Maximum markup items (`<` markers) in the input. |
| `IRONFLOW_MAX_EXTRACT_OUTPUT_BYTES` | `52428800` (50 MiB) | Maximum Markdown size, charged while converting; nested quotes, lists and padded tables count toward it. |

Exceeding a limit fails the step.

## Examples

### Convert a fetched page

```lua
local flow = Flow.new("convert_html")

flow:step("fetch", nodes.http_request({
    url = "https://example.com/page",
    output_key = "html_content"
}))

flow:step("convert", nodes.html_to_markdown({
    source_key = "html_content",
    output_key = "markdown"
})):depends_on("fetch")

flow:step("done", nodes.log({
    message = "Markdown result: ${ctx.markdown}"
})):depends_on("convert")

return flow
```
