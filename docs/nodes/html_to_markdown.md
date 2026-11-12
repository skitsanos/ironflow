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

## Resource and cancellation contract

The node admits its input like [`extract_html`](extract_html.md) before any
HTML is parsed:

- The HTML string may be at most `IRONFLOW_MAX_FILE_BYTES` (default
  `52428800`, 50 MiB). A larger input fails with `html_to_markdown: input is N
  bytes, exceeds IRONFLOW_MAX_FILE_BYTES (LIMIT)`.
- `IRONFLOW_MAX_EXTRACT_ITEMS` (default `250000`) bounds the markup items in
  the input, counted as `<` markers.
- Elements may nest at most 128 levels deep. The cap is fixed, not
  configurable, and counts the `<html>` and `<body>` elements the parser
  supplies around a fragment, so a fragment may nest 126 elements of its own.
  Inert `<template>` content counts as nested inside its template element. A
  deeper input fails with `html_to_markdown: HTML element nesting exceeds 128
  levels` as an ordinary node error; the converter recurses once per level and
  would otherwise exhaust the worker stack and abort the process. Between
  4 KiB pieces of input, parsing stops once an open element is more than 256
  levels deep, and the finished tree is checked against the exact cap before
  conversion.
- The output is not charged against `IRONFLOW_MAX_EXTRACT_OUTPUT_BYTES`; the
  usual `IRONFLOW_MAX_TASK_OUTPUT_BYTES` persistence limit applies afterwards.

Parsing and conversion run on a tracked blocking worker, not on the async
runtime. HTML is fed to the parser in 4 KiB pieces with a cancellation and
step/run deadline check between them. The third-party Markdown conversion
cannot be interrupted internally; IronFlow checks before and after it. Task
and run admission remain occupied until the physical worker stops.

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
