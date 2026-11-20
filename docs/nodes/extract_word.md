# `extract_word`

Extract text, Markdown, or a structured block list from a Word (`.docx`) document, with optional metadata, comments, notes, headers and footers.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | One of `path`/`source_key` | — | Path to the `.docx` file; supports `${ctx.key}` interpolation. |
| `source_key` | string | One of `path`/`source_key` | — | Context key holding a file path, artifact URI, or artifact descriptor; the stored path is not interpolated. |
| `format` | string | No | `"text"` | `"text"`, `"markdown"`, or `"json"`; any other value fails the step. |
| `output_key` | string | No | `"content"` | Context key for the result (string for text/Markdown, object for JSON). |
| `metadata_key` | string | No | — | When set, store document properties under this key. |
| `comments_key` | string | No | — | When set, store document comments under this key. |
| `notes` | boolean | No | `true` | Mark footnote and endnote references and list the referenced notes after the body; `false` reads no notes. |
| `headers_footers` | boolean | No | `false` | Also extract the header and footer parts the document's sections reference. |

`path` and `source_key` are mutually exclusive. `output_key`, `metadata_key` and `comments_key` must name different keys. String parameters of the wrong type fail the step. `notes` and `headers_footers` accept a boolean or an interpolated `true`/`false`, `yes`/`no`, `on`/`off`, `1`/`0`.

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}` | string or object | Extracted text, Markdown, or JSON blocks. |
| `{metadata_key}` | object | Available fields of `title`, `author`, `subject`, `description`, `keywords`, `last_modified_by`, `created`, `modified`, `revision`, `category`; `{}` when the document has no core properties. |
| `{comments_key}` | array | Comments as below; `[]` when the document has none. |

## Output Shape

`format = "json"` returns body blocks (paragraphs and tables) in document order, plus note and header/footer arrays that are present only when they have entries:

```json
{
  "blocks": [
    {
      "type": "paragraph", "index": 0, "style": "Heading1",
      "list": { "level": 0, "numbered": true },
      "colors": ["0066FF"],
      "runs": [
        { "text": "MODERATOR SAY: ", "bold": true, "color": "0066FF" },
        { "text": "See note[^1]", "note": { "type": "footnote", "label": "1" } }
      ],
      "text": "MODERATOR SAY: See note[^1]"
    },
    { "type": "table", "index": 1, "rows": [ { "cells": [ { "paragraphs": [] } ] } ] }
  ],
  "footnotes": [ { "label": "1", "id": "6857", "blocks": [] } ],
  "endnotes": [ { "label": "e1", "id": "3", "blocks": [] } ],
  "headers": [ { "type": "default", "part": "word/header.xml", "blocks": [] } ],
  "footers": [ { "type": "default", "part": "word/footer.xml", "blocks": [] } ]
}
```

- Run flags `bold`, `italic`, `underline`, `strike` appear only when true; `color` is uppercase hex without `#`; `highlight` is the Word highlight name; `style`, `list` and `colors` appear only when they apply.
- `note.mark` (on the run and the note) holds a custom footnote mark such as `*`.
- Header and footer `type` is the section slot (`default`, `first`, `even`) of the first reference.

Each comment has `id`, `text`, and when present `author`, `initials`, `date`, and `anchored_text` (the text between the comment's range markers):

```json
[{ "id": "1", "author": "Jane Reviewer", "initials": "JR", "date": "2026-03-15T10:30:00Z",
   "text": "Reword this.", "anchored_text": "quick brown fox" }]
```

## Behavior

- Footnote references become `[^1]`, `[^2]`, ... and endnote references `[^e1]`, ... in reference order; each referenced note is listed once after the body as `[^label]: text`, footnotes first. Unreferenced notes are not returned. A custom mark moves to JSON `mark`; with `notes = false` it stays in the text.
- Text and Markdown order: headers, body, notes, footers, separated by blank lines. Header and footer parts without text are left out.
- Bold, italic and strikethrough are resolved through paragraph and character styles; `underline`, `color` and `highlight` come from the run only; table styles are not applied. Tracked formatting changes count as accepted.
- Theme colors resolve to hex (tint and shade not applied); `auto` gives no color.
- Markdown merges adjacent runs with the same emphasis into one span, and a heading does not repeat the bold or italic of its own style.
- Text box content follows its anchoring paragraph as separate blocks, once. Nested tables are flattened into the surrounding cell.
- An absent optional part (styles, numbering, theme, notes, comments, properties, header, footer) gives empty data. A malformed part, a DTD, an unknown entity, or an external or escaping package relationship fails the step.

## Limits

| Variable | Default | Effect |
|----------|---------|--------|
| `IRONFLOW_MAX_FILE_BYTES` | `52428800` | Maximum input file size. |
| `IRONFLOW_MAX_ZIP_ENTRIES` | `10000` | Maximum entries in the package. |
| `IRONFLOW_MAX_ZIP_UNCOMPRESSED_BYTES` | `536870912` | Maximum total declared uncompressed size. |
| `IRONFLOW_MAX_EXTRACT_ITEMS` | `250000` | XML events and parsed items per call. |
| `IRONFLOW_MAX_EXTRACT_OUTPUT_BYTES` | `52428800` | Maximum serialized result, content plus metadata and comments. |

Archive validation rules are in [SECURITY.md](../SECURITY.md).

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

### JSON blocks with comments

```lua
flow:step("extract", nodes.extract_word({
    path = "${ctx.source}",
    format = "json",
    output_key = "doc",
    comments_key = "comments"
}))
```
