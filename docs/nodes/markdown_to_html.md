# `markdown_to_html`

Convert Markdown to HTML using CommonMark with the GFM strikethrough, table, autolink and task-list extensions.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `input` | string | One of `input` or `source_key` | — | Literal Markdown; supports `${ctx.key}` interpolation. |
| `source_key` | string | One of `input` or `source_key` | — | Context key holding the Markdown; a non-string value is serialized as JSON first. |
| `output_key` | string | No | `"html"` | Context key for the HTML. |
| `sanitize` | boolean | No | `false` | When `true`, clean the HTML with ammonia's default allow-list, as [`html_sanitize`](html_sanitize.md) does with no options. |

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}` | string | Generated HTML. |

## Behavior

- Setting both `input` and `source_key`, or neither, fails the step; so does a `source_key` that is not in the context.
- A timed-out or cancelled step finishes only after the conversion returns; the conversion itself cannot be interrupted.

## Examples

### Render and sanitize

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
