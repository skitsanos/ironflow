# `html_to_markdown`

Convert HTML to Markdown (best-effort, inherently lossy on complex HTML).

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `input` | string | one of `input` or `source_key` | — | Literal HTML string; supports `${ctx.*}` interpolation. |
| `source_key` | string | one of `input` or `source_key` | — | Context key whose value contains the HTML text. |
| `output_key` | string | no | `"markdown"` | Context key where the resulting Markdown is stored. |

> Providing both `input` and `source_key` is an error.

## Context Output

- `<output_key>` (default `markdown`) — the generated Markdown string.

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
