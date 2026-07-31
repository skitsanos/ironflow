# `extract_word`

Extract text, metadata, or a structured representation from a Word (.docx) document.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | one of `path` or `source_key` | — | File path to the `.docx` file; supports `${ctx.key}` interpolation. |
| `source_key` | string | one of `path` or `source_key` | — | Context key containing a file path, artifact URI, or artifact descriptor. |
| `format` | string | no | `"text"` | Output format: `"text"` for plain text, `"markdown"` for Markdown with headings/lists/inline formatting, `"json"` for a structured block IR (see below). |
| `output_key` | string | no | `"content"` | Context key where the extracted output is stored. For `text`/`markdown` the value is a string; for `json` it is an object. |
| `metadata_key` | string | no | — | If set, document metadata (Dublin Core fields) is stored under this context key. |
| `comments_key` | string | no | — | If set, document comments (from `word/comments.xml`) are stored under this context key as an array. See [Comments](#comments). |

> Providing both `path` and `source_key` is an error.
> The `format` parameter accepts `"text"`, `"markdown"`, or `"json"`; any other value is rejected.
> Present `format`, `output_key`, `metadata_key`, and `comments_key` values must be strings; a value of the wrong type is rejected instead of being treated as absent.
> `output_key`, `metadata_key`, and `comments_key` must be pairwise distinct when the optional keys are set; collisions are rejected before extraction begins.

## Context Output

- `<output_key>` (default `content`) — the extracted document text, Markdown, or JSON IR.
- `<metadata_key>` (only when `metadata_key` is set) — an object with available fields: `title`, `author`, `subject`, `description`, `keywords`, `last_modified_by`, `created`, `modified`, `revision`, `category`.
- `<comments_key>` (only when `comments_key` is set) — the comments array described below; an absent comments part produces an empty array.

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

## Resource and cancellation contract

- The input must be a regular file. Its raw size and central directory are
  bounded by `IRONFLOW_MAX_FILE_BYTES` (default `52428800`, 50 MiB) before the
  ZIP library allocates archive metadata. On Unix, IronFlow also refuses to
  follow a final path-component symlink; other platforms enforce the
  opened-handle regular-file check.
- The EOCD/ZIP64 preflight validates central-directory bounds and
  `IRONFLOW_MAX_ZIP_ENTRIES` (default `10000`). IronFlow then rejects duplicate
  part names, symlink or special-file parts, and a cumulative declared
  uncompressed size above `IRONFLOW_MAX_ZIP_UNCOMPRESSED_BYTES` (default
  `536870912`, 512 MiB).
- XML parts are parsed directly from bounded ZIP-entry readers; IronFlow does
  not first materialize a complete part as a byte vector or string. Each part
  is capped by the smaller of the remaining cumulative ZIP budget and the
  extraction-output budget, and actual decoded bytes accumulate across every
  part read. Every present XML part used by the node must be well-formed, have
  one root element, contain no DTD, and have valid attributes. A malformed,
  non-UTF-8, oversized, or unreadable present part is an error. Genuinely
  missing optional numbering, theme, requested metadata, or requested comments
  parts use empty/default data; failures are not converted into missing parts
  or partial output. `quick_xml` still buffers its current event, so one
  unusually large text or CDATA token can require memory proportional to that
  token, but not to the complete XML part.
- `IRONFLOW_MAX_EXTRACT_ITEMS` (default `250000`) is shared across the call. It
  counts DOCX XML events in document, numbering, theme, metadata, and comments
  parsing, inspected attributes, parsed comments, list-indent formatting work,
  and each open-comment-range/text-event fan-out. This bounds adversarial
  overlapping comment ranges as well as ordinary document structure.
- `IRONFLOW_MAX_EXTRACT_OUTPUT_BYTES` (default `52428800`, 50 MiB) bounds the
  complete serialized `NodeOutput`: content plus requested metadata and
  comments. Retained text, metadata, comments, anchors, and formatting growth
  are charged incrementally before allocation where possible; the bounded
  final serialization checks the complete result. The setting also
  participates in the per-part decoded ceiling. This is a logical result
  limit, not a process-RSS limit and not the later
  `IRONFLOW_MAX_TASK_OUTPUT_BYTES` persistence limit.
- ZIP and XML work runs on a tracked blocking worker. Archive reads and parser
  loops check cancellation and the step/run deadline between chunks and XML
  events; synchronous ZIP-library construction cannot be interrupted inside
  the call and is bracketed by checkpoints. Task and run admission remain
  occupied until the physical worker stops.

## Examples

### Markdown extraction

```lua
local flow = Flow.new("read_word_doc")

flow:step("extract", nodes.extract_word({
    path = "/data/report.docx",
    format = "markdown",
    output_key = "doc_content",
    metadata_key = "doc_meta"
}))

flow:step("done", nodes.log({
    message = "Extracted Word document by ${ctx.doc_meta.author}"
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
