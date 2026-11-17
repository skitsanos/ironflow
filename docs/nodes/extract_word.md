# `extract_word`

Extract text, metadata, or a structured representation from a Word (.docx) document.

## Text Fidelity

Text extraction decodes built-in XML entities and numeric character references
once and preserves literal CDATA, including in metadata, comments, and comment
anchors. Adjacent text events and runs are concatenated without invented spaces;
comment paragraphs are separated by newlines. Body text is read from `w:t`,
not XML indentation inside a run. XML attributes, including IDs, styles and
author names, are normalized using the part's declared XML version (1.0 by
default). Metadata outer whitespace is trimmed after text accumulation.

Unknown/invalid references and DTDs are errors, not silently dropped content.
No custom or external entity expansion is performed. Decoded text is admitted
against the existing extraction-output budget before allocation/retention;
temporary attribute normalization is checked against available output capacity.
ZIP byte/entry, XML event/item, deadline, and final serialized-output checks
remain in force. There are no new configuration options.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | one of `path` or `source_key` | — | File path to the `.docx` file; supports `${ctx.key}` interpolation. |
| `source_key` | string | one of `path` or `source_key` | — | Context key containing a file path, artifact URI, or artifact descriptor. A stored path is used as it is and is not interpolated. |
| `format` | string | no | `"text"` | Output format: `"text"` for plain text, `"markdown"` for Markdown with headings/lists/inline formatting, `"json"` for a structured block IR (see below). |
| `output_key` | string | no | `"content"` | Context key where the extracted output is stored. For `text`/`markdown` the value is a string; for `json` it is an object. |
| `metadata_key` | string | no | — | If set, document metadata (Dublin Core fields) is stored under this context key. |
| `comments_key` | string | no | — | If set, document comments (from `word/comments.xml`) are stored under this context key as an array. See [Comments](#comments). |
| `notes` | boolean | no | `true` | Mark footnote and endnote references in the text and list the referenced notes after the body. `false` leaves no marker and reads no notes part. See [Footnotes, endnotes, headers and footers](#footnotes-endnotes-headers-and-footers). |
| `headers_footers` | boolean | no | `false` | Also extract the header and footer parts that the document's sections name. Off by default because they are page furniture that repeats on every page. |

> Providing both `path` and `source_key` is an error.
> Artifact inputs are opened and SHA-256 verified inside the tracked blocking worker; OOXML parsing consumes that same rewound handle rather than a resolved store pathname.
> The `format` parameter accepts `"text"`, `"markdown"`, or `"json"`; any other value is rejected.
> Present `format`, `output_key`, `metadata_key`, and `comments_key` values must be strings; a value of the wrong type is rejected instead of being treated as absent. `notes` and `headers_footers` take a native boolean or an interpolated boolean spelling (`true`/`false`, `yes`/`no`, `on`/`off`, `1`/`0`).
> `output_key`, `metadata_key`, and `comments_key` must be pairwise distinct when the optional keys are set; collisions are rejected before extraction begins.

## Context Output

- `<output_key>` (default `content`) — the extracted document text, Markdown, or JSON IR.
- `<metadata_key>` (only when `metadata_key` is set) — an object with available fields: `title`, `author`, `subject`, `description`, `keywords`, `last_modified_by`, `created`, `modified`, `revision`, `category`. The core-properties part is the target of the package relationship of type `http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties` in `_rels/.rels` (Office for the web names `package/services/metadata/core-properties/<id>.psmdcp`); `docProps/core.xml` is read only when the package has no such relationship. The target must be internal and resolve inside the package; an external, escaping or duplicated core-properties relationship fails the step, and a target part that is absent gives `{}`. `_rels/.rels` is read only when `metadata_key` is set. Fields are matched by namespace and local name (Dublin Core `title`, `creator`, `subject`, `description`; DCMI terms `created`, `modified`; core-properties `keywords`, `lastModifiedBy`, `revision`, `category`), not by prefix, so an unprefixed `lastModifiedBy` in the default namespace is read and an element from another namespace is ignored.
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

Run flags (`bold`, `italic`, `underline`, `strike`) are emitted only when true. A toggle property counts as on when it is present without `w:val` or with `1`, `true` or `on`, and as off with `0`, `false` or `off`; `w:u` is off for `w:val="none"`. Tracked formatting changes are read as accepted: the previous properties stored under `w:rPrChange` and `w:pPrChange` are ignored, so a run or paragraph reports only its current formatting, style and list state. `color` is the resolved hex (uppercase, no `#`) — see below. `highlight` is the OOXML highlight name when set (e.g. `"yellow"`). `style` and `list` are absent when not applicable. `colors` is a deduped union of run colors at the paragraph level, present only when at least one run has a color.

### Style inheritance of bold, italic and strikethrough

`bold`, `italic` and `strike` are toggle properties and are resolved through
`word/styles.xml` as WordprocessingML defines them:

1. Along each `w:basedOn` chain the nearest value wins, so a derived style
   with `<w:i w:val="0"/>` switches off the italic of its base.
2. The result for the paragraph style (`w:pStyle`, or the default paragraph
   style) and for the character style (`w:rStyle`, or the default character
   style) are combined by exclusive or: a bold character style inside a bold
   paragraph style is not bold.
3. A toggle that the document defaults (`w:docDefaults`) turn on stays on
   whatever the styles say.
4. A toggle set directly on the run (`<w:b/>`, `<w:b w:val="0"/>`) replaces
   all of the above.

A style reference that names no style of the right kind falls back to the
default style. A `w:basedOn` cycle ends the chain. When `word/styles.xml`
defines one `w:styleId` more than once (some generators write their built-in
styles first and the document's own after them), the last definition is
used, as Word does. Table styles and their
conditional formatting (for example a bold first row) are not resolved, and
neither are `underline`, `color` and `highlight`, which are reported from the
run's own properties only. The styles part is parsed under the same
`IRONFLOW_MAX_EXTRACT_ITEMS` and output budgets as the document. A package
without `word/styles.xml` reports direct formatting only.

In Markdown a heading does not repeat the emphasis of its own style: a run in
a `Heading1` paragraph whose style is bold renders as `# Title`, not
`# **Title**`, while emphasis that differs from the style (an italic run in
that heading) keeps its markers. JSON reports the resolved flags on every run.

### Markdown emphasis

Word splits text into runs at arbitrary points (spell-check, revisions,
editing in Office for the web), so Markdown treats a paragraph's runs as one
stream rather than wrapping each run on its own:

- Adjacent runs with the same bold, italic and strikethrough share one marker
  pair: an italic run `finished` followed by an italic run ` units` renders as
  `*finished units*`.
- Leading and trailing whitespace stays outside the markers (`It **expects** a
  value.`), and a run that holds only whitespace never opens a marker. A space
  between two runs with the same emphasis joins the span.
- When the emphasis changes, only the markers that end are closed, so the rest
  stays nested: `**strong *both* strong**`. Bold and italic together open as
  `***`; strikethrough is `~~` inside them.
- Table cells apply the same rules and escape `|` as `\|`; headings leave out
  the emphasis of their own style as described above.

CommonMark cannot express two emphasis spans that overlap without a whitespace
boundary (bold that ends in the middle of a word where italic continues);
such a boundary is written as a close and reopen and may render with a literal
`*`. JSON keeps every run and its flags unchanged.

### Text boxes

A text box is stored inside a run of the paragraph that anchors it. The
anchoring paragraph keeps all of its own text, and the paragraphs of the box
follow it as separate blocks in every format, each with its own style and
runs. Empty paragraphs inside a box are omitted. A table inside a box follows
the anchoring paragraph as a table block. When the anchoring paragraph sits in
a table cell, the box content stays in that cell as further paragraphs. Each
box is emitted once: the `mc:Fallback` copy that word processors write next to
the `mc:Choice` content is skipped, and with it any other fallback content.
Boxes nested in boxes are flattened in the same order. The position of the box
on the page is not used.

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

## Footnotes, endnotes, headers and footers

Footnotes and endnotes are extracted by default. A reference in the body
(`w:footnoteReference`, `w:endnoteReference`) becomes the marker `[^label]` at
its position. Labels count in reference order and are not the `w:id`:
footnotes are `1`, `2`, ..., endnotes `e1`, `e2`, ...; a note referenced again
reuses its label. References inside a tracked deletion (`w:del`) or an
`mc:Fallback` mark nothing. After the body, each referenced note is listed
once, footnotes first, as `[^label]: text`, its paragraphs (table cells
included) joined by a space; that is the footnote syntax of GitHub Flavored
Markdown, and the same lines are used in text. Notes nobody references, such
as the separator notes, are not returned. A reference whose note or notes part
is missing keeps its marker without a definition.

With `headers_footers = true`, the header and footer parts named by the
`w:headerReference` and `w:footerReference` elements of every section are
read, each part once in the order first referenced (`default`, `first` and
`even` slots alike). A part without text is left out, and references inside a
tracked section change (`w:sectPrChange`) are ignored. Text and Markdown put
the headers before the body and the footers at the end, after the notes.

Text and Markdown separate these sections with a blank line: headers, body,
notes, footers. JSON keeps them apart:

```json
{
  "blocks": [ /* body blocks; a referencing run has "note" */ ],
  "footnotes": [ { "label": "1", "id": "6857", "blocks": [ /* paragraph and table blocks */ ] } ],
  "endnotes": [ { "label": "e1", "id": "3", "blocks": [ /* ... */ ] } ],
  "headers": [ { "type": "default", "part": "word/header.xml", "blocks": [ /* ... */ ] } ],
  "footers": [ { "type": "default", "part": "word/footer.xml", "blocks": [ /* ... */ ] } ]
}
```

Each array is present only when it has entries. A run that holds a reference
carries `"note": {"type": "footnote" | "endnote", "label": "1"}` and its text
includes the marker, as does the paragraph `text`. `type` on a header or
footer is the slot of its first reference.

Parts are found through `word/_rels/document.xml.rels` (Office for the web
writes `header.xml` and `footer.xml`, not `header1.xml`); the notes parts fall
back to `word/footnotes.xml` and `word/endnotes.xml` when the document has no
such relationship. Targets are untrusted: an external, escaping or wrong-type
relationship, or more than one footnotes or endnotes relationship, fails the
step, while a relationship that names a missing part, or a section reference
with no relationship, is skipped. These parts share the call's ZIP, XML event
and output budgets, and only referenced notes are kept in memory.

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

Comment ranges and text inside an `mc:Fallback` element are not observed, consistently with the document text: the fallback copy of a text box repeats the `mc:Choice` content, so a range around or inside a text box anchors the box text once. A range that only a fallback declares anchors nothing.

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
  ZIP library allocates archive metadata. On Unix and Windows, IronFlow also
  refuses to follow a final path-component symlink/reparse point; other platforms enforce the
  opened-handle regular-file check.
- The EOCD/ZIP64 preflight validates central-directory bounds and
  `IRONFLOW_MAX_ZIP_ENTRIES` (default `10000`). It accepts only the last end
  record in the file, which must end at EOF with its comment, and a central
  directory that ends exactly where that record begins; the opened archive's
  directory offset and raw entry count must then match the validated ones.
  Packages with trailing bytes, prepended data, or a second end record inside
  the archive comment are rejected. IronFlow then rejects duplicate
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
  missing optional numbering, theme, requested metadata, notes, header,
  footer, or requested comments parts use empty/default data; failures are not converted into missing parts
  or partial output. `quick_xml` still buffers its current event, so one
  unusually large text or CDATA token can require memory proportional to that
  token, but not to the complete XML part.
- `IRONFLOW_MAX_EXTRACT_ITEMS` (default `250000`) is shared across the call. It
  counts DOCX XML events in document, numbering, theme, metadata, notes,
  header, footer and comments parsing, inspected attributes, parsed comments, list-indent formatting work,
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
