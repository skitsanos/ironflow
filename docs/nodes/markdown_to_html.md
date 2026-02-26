# `markdown_to_html`

Convert Markdown text to HTML using CommonMark + GFM extensions (strikethrough, tables, autolinks, task lists).

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `input` | string | one of `input` or `source_key` | — | Literal Markdown string; supports `${ctx.*}` interpolation. |
| `source_key` | string | one of `input` or `source_key` | — | Context key whose value contains the Markdown text. |
| `output_key` | string | no | `"html"` | Context key where the resulting HTML is stored. |
| `sanitize` | bool | no | `false` | When `true`, sanitize the HTML output with ammonia to remove unsafe tags/attributes. |

> Providing both `input` and `source_key` is an error.

## Context Output

- `<output_key>` (default `html`) — the generated HTML string.

## Example

```lua
local flow = Flow.new("convert_markdown")

flow:step("render", nodes.markdown_to_html({
    input = "# Hello\n\nThis is **bold** and ~~struck~~.",
    sanitize = true,
    output_key = "html"
}))

flow:step("done", nodes.log({
    message = "HTML output: ${ctx.html}"
})):depends_on("render")

return flow
```
