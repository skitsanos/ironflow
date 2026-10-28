# `html_to_markdown`

Convert HTML to Markdown (best-effort, inherently lossy on complex HTML).

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `input` | string | one of `input` or `source_key` | — | Literal HTML string; supports `${ctx.key}` interpolation. |
| `source_key` | string | one of `input` or `source_key` | — | Context key whose value contains the HTML text. |
| `output_key` | string | no | `"markdown"` | Context key where the resulting Markdown is stored. |

> Providing both `input` and `source_key` is an error.

## Context Output

- `<output_key>` (default `markdown`) — the generated Markdown string.

## Content Filtering

The input is parsed structurally with the HTML parser's recovery rules, so
fragments and malformed markup are accepted. `head`, `title`, `script`,
`style`, `noscript`, and inert `template` subtrees are skipped, the same set
that [`extract_html`](extract_html.md) removes, so a page title or style sheet
never surfaces as Markdown text. Comments are omitted. Headings, emphasis,
links, images, inline and fenced code, block quotes, lists, and tables (GFM
pipe tables) are retained; literal Markdown punctuation in prose is escaped
(`# literal` becomes `\# literal`) so it still renders as text, while code
content is left untouched. Conversion is best-effort: CSS, scripts, and
external resources are never evaluated, and this is not a sanitizer.

## Example

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
