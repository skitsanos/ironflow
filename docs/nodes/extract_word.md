# `extract_word`

Extract text, metadata, or a structured representation from a Word (.docx) document.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | one of `path` or `source_key` | — | File path to the `.docx` file; supports `${ctx.*}` interpolation. |
| `source_key` | string | one of `path` or `source_key` | — | Context key whose value is the file path (must be a string). |
| `format` | string | no | `"text"` | Output format: `"text"` for plain text, `"markdown"` for Markdown with headings/lists/inline formatting, `"json"` for a structured block IR (see below). |
| `output_key` | string | no | `"content"` | Context key where the extracted output is stored. For `text`/`markdown` the value is a string; for `json` it is an object. |
| `metadata_key` | string | no | — | If set, document metadata (Dublin Core fields) is stored under this context key. |
| `comments_key` | string | no | — | If set, document comments (from `word/comments.xml`) are stored under this context key as an array. See [Comments](#comments). |

> Providing both `path` and `source_key` is an error.
> The `format` parameter accepts `"text"`, `"markdown"`, or `"json"`; any other value is rejected.

## Context Output

- `<output_key>` (default `content`) — the extracted document text, Markdown, or JSON IR.
- `<metadata_key>` (only when `metadata_key` is set) — an object with available fields: `title`, `author`, `subject`, `description`, `keywords`, `last_modified_by`, `created`, `modified`, `revision`, `category`.

## JSON format

When `format = "json"`, the output is an object with a single `blocks` array. Each block is either a paragraph or a table, in source document order. This shape is designed for downstream LLM extraction with `response_format = json_schema` — it preserves run-level styling, colors (including resolved theme colors), and table structure, all of which are commonly load-bearing semantics in real-world documents (e.g. color-coded moderator instructions in market-research discussion guides).

### Paragraph block

```json
{
  "type": "paragraph",
  "index": 0,
  "style": "Heading1",
  "list": { "level": 0, "numbered": true },
  "colors": ["0066FF"],
  "runs": [
    { "text": "MODERATOR SAY: ", "bold": true, "color": "0066FF" },
    { "text": "Thank you for joining today.", "color": "0066FF" }
  ],
  "text": "MODERATOR SAY: Thank you for joining today."
}
```

Run flags (`bold`, `italic`, `underline`, `strike`) are emitted only when true. `color` is the resolved hex (uppercase, no `#`) — see below. `highlight` is the OOXML highlight name when set (e.g. `"yellow"`). `style` and `list` are absent when not applicable. `colors` is a deduped union of run colors at the paragraph level, present only when at least one run has a color.

### Table block

```json
{
  "type": "table",
  "index": 1,
  "rows": [
    { "cells": [
        { "paragraphs": [ /* paragraph blocks as above */ ] },
        { "paragraphs": [ /* ... */ ] }
    ] }
  ]
}
```

Tables nested inside cells are flattened — inner paragraphs are appended to the surrounding cell's paragraph list; no nested table block is emitted.

## Comments

When `comments_key` is set, the node also parses `word/comments.xml` (if present) and walks `word/document.xml` for `<w:commentRangeStart/End>` markers to capture the source text each comment is anchored to.

Shape of `ctx[comments_key]`:

```json
[
  {
    "id": "1",
    "author": "Jane Reviewer",
    "initials": "JR",
    "date": "2026-03-15T10:30:00Z",
    "text": "Reword this — too colloquial.",
    "anchored_text": "quick brown fox"
  }
]
```

`anchored_text` is the verbatim text the comment was attached to (joined run text between the comment's `<w:commentRangeStart/>` and `<w:commentRangeEnd/>` markers). It is omitted when the comment has no anchored range (whole-document comments).

If the document has no comments part, an empty array is written.

```lua
flow:step("extract", nodes.extract_word({
    path = "${ctx.source}",
    format = "json",
    output_key = "doc",
    comments_key = "comments"
}))
```

### Color resolution

Run color is captured from `w:color`:
- Explicit hex (`w:val="0066FF"`) is captured as uppercase hex.
- `w:val="auto"` is dropped (no color field).
- `w:themeColor` is resolved against `word/theme/theme1.xml`: theme names `dark1`, `light1`, `dark2`, `light2`, `accent1`...`accent6`, `hyperlink`, `followedHyperlink` are mapped to their concrete `srgbClr` / `sysClr` hex values from the document's color scheme. `themeShade` / `themeTint` adjustments are not currently applied.

## Examples

### Markdown extraction (default)

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

### JSON IR for LLM-driven extraction

```lua
local flow = Flow.new("guide_to_json")

flow:step("extract", nodes.extract_word({
    path = "${ctx.source_path}",
    format = "json",
    output_key = "doc"
}))

flow:step("analyze", nodes.llm({
    provider = "openai",
    model = "gpt-5-mini",
    prompt = "Convert the following discussion guide IR into the project schema. Use color hints (0066FF = MODERATOR SAY, 037C72 = MODERATOR NOTE, etc.). IR: ${ctx.doc}",
    output_key = "guide",
    extra = {
        response_format = {
            type = "json_schema",
            json_schema = {
                name = "discussion_guide",
                strict = true,
                schema = {
                    -- your guide schema here
                }
            }
        }
    }
})):depends_on("extract")

return flow
```
