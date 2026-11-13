# `extract_pdf`

Extract text and metadata from a PDF document.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | one of `path` or `source_key` | — | File path to the PDF; supports `${ctx.key}` interpolation. |
| `source_key` | string | one of `path` or `source_key` | — | Context key containing a file path, artifact URI, or artifact descriptor. A stored path is used as it is and is not interpolated. |
| `format` | string | no | `"text"` | Output format: `"text"` for raw extracted text, `"markdown"` for best-effort paragraph-grouped Markdown. |
| `output_key` | string | no | `"content"` | Context key where the extracted text is stored. |
| `metadata_key` | string | no | — | If set, PDF metadata is stored under this context key. |

> Providing both `path` and `source_key` is an error.
> Artifact inputs are opened and SHA-256 verified inside the tracked blocking worker; PDF parsing consumes that same rewound handle rather than a resolved store pathname.
> The `format` parameter only accepts `"text"` or `"markdown"`; any other value is rejected.
> Present `format`, `output_key`, and `metadata_key` values must be strings; a value of the wrong type is rejected instead of being treated as absent.
> When `metadata_key` is set, it must differ from `output_key`; key collisions are rejected before extraction begins.

## Context Output

- `<output_key>` (default `content`) — the extracted text or Markdown.
- `<metadata_key>` (only when `metadata_key` is set) — an object with available fields: `pages` (number), `title`, `author`, `subject`, `keywords`, `creator`, `producer`, `created`, `modified`.

If `metadata_key` is requested and a present PDF Info entry or supported field
cannot be resolved as the expected type, extraction fails rather than silently
returning incomplete metadata.

Metadata text fields are decoded as PDF text strings, exactly as
[`pdf_metadata`](pdf_metadata.md) does: PDFDocEncoding, UTF-16BE with a
byte-order mark and UTF-8 with a byte-order mark. Values are trimmed; NUL and
other control characters except tab, line feed and carriage return are
removed, together with language-tag escapes. An ill-formed UTF-16 or UTF-8
sequence becomes U+FFFD instead of failing the step, and a field that is empty
after decoding is omitted.

PDF content streams position glyphs rather than encode semantic words and
paragraphs. Text and Markdown extraction preserve decoded characters on a
best-effort basis, but whitespace can be introduced or omitted when positioned
glyph fragments are joined. Do not treat exact whitespace or byte-for-byte
output as a stable document-identity representation.

When page marked-content sequences supply `/ActualText`, extraction uses that
logical replacement string once instead of the enclosed glyphs. The outermost
replacement wins for nested spans; an empty string deliberately suppresses the
enclosed content. Direct dictionaries and named `/Properties` resources,
including inherited resources and indirect property/text references, are
supported. PDFDocEncoding, BOM-marked UTF-16BE and BOM-marked UTF-8 text strings
are decoded with lopdf's text-string APIs. Malformed strings/properties,
unbalanced markers and cyclic references fail instead of returning partial text.

This preserves logical Hebrew/Arabic, combining marks, CJK and mixed-direction
text in PDFs produced by `markdown_to_pdf` ([IF-156](../issues/IF-156.md)). It is
not OCR or a general semantic-layout reconstruction. Structure-tree-only
ActualText and text inside Form XObjects are not newly traversed; untagged text
still depends on the existing font encodings and stream order. No heuristic
string reversal is applied. `extract_pdf` still reads all pages of its input;
use `pdf_split` first to select pages, preserving their replacement resources.
Replacement text is author-supplied and can differ from what is visibly drawn;
extraction does not verify semantic consistency or prove that a PDF was redacted
correctly.

## Resource and cancellation contract

- The input must be a regular file. `IRONFLOW_MAX_PDF_BYTES` (default
  `104857600`, 100 MiB) bounds both its declared size and actual bytes read. On
  Unix, IronFlow also refuses to follow a final path-component symlink; other
  platforms enforce the opened-handle regular-file check.
- `IRONFLOW_MAX_PDF_EXTRACT_PAGES` (default `1000`) rejects the document after
  its page tree is parsed but before text extraction begins.
- [Shared PDF loading](../PDF_LOADING.md) limits each object/cross-reference
  stream to `IRONFLOW_MAX_PDF_DECOMPRESSED_STREAM_BYTES` (64 MiB) and rejects
  more than `IRONFLOW_MAX_PDF_OBJECTS` (250000) loaded objects after parsing.
  Strict parsing and bounded recovery checks reject oversized streams instead
  of returning partial success. These are not a total memory ceiling.
- `IRONFLOW_MAX_EXTRACT_ITEMS` (default `250000`) is cumulative across PDF
  pages, present supported metadata fields, fonts, content operations, text
  operands, property-reference/resource-ancestor hops, 4 KiB input chunks of
  replacement/glyph strings, and extracted text lines.
- `IRONFLOW_MAX_EXTRACT_OUTPUT_BYTES` (default `52428800`, 50 MiB) bounds the
  extracted text before it is appended and the complete serialized
  `NodeOutput`, including content and requested metadata. The remaining text
  budget also bounds each page's decompressed content stream and each
  font-mapping stream when its font is first used.
  This is not the later `IRONFLOW_MAX_TASK_OUTPUT_BYTES` persistence limit.
- `IRONFLOW_MAX_PDF_EXTRACT_CMAP_ENTRIES` (default `1048576`) bounds the
  ToUnicode mappings of all fonts in one call ([IF-178](../issues/IF-178.md)).
  Before `lopdf` parses a font's ToUnicode CMap, IronFlow scans the decoded
  stream and charges each `bfchar` entry at least the UTF-16 units of its
  destination and each `bfrange` line the number of codes it covers multiplied
  by the UTF-16 units of its longest destination; a codespace line adds one or
  two. The count is cumulative over distinct font dictionaries, so a range
  such as `<00000001> <FFFFFFFE>` fails the node in the time it takes to read
  the stream instead of being expanded. A font is charged and decoded once,
  with the first page whose resources list it, however many pages share it; a
  ToUnicode stream is charged even if the font's `/Encoding` means it is never
  consulted. The scan is deliberately conservative: a range line inside a
  comment of a `bfrange` section is charged too, a `bfchar` destination that
  contains a comment is charged as 256 units, and a `bfrange` destination that
  contains a comment cannot be measured and fails the font.
- A glyph string of `n` bytes is admitted by its worst case before it is
  decoded: `3n` UTF-8 bytes in a single-byte or UTF-16 font, and `6n` times
  the UTF-16 units of the longest ToUnicode destination in a code-mapped
  font. A string whose worst case is at most 1 MiB, or fits the remaining
  output budget, is decoded in one call and its actual text is then admitted
  exactly. A larger string is decoded in pieces of at most 1 MiB worst case
  when its font decodes each byte on its own; a code-mapped or UTF-16 string
  cannot be split safely and is rejected with the output-limit error, even if
  its actual text would have fitted.
- Marked-content and text-operand nesting are limited to 64 levels. Property
  reference chains are limited to 32 hops with cycle detection; named-property
  resource inheritance is limited to 64 ancestors. ActualText's decoded UTF-8
  length is checked before the replacement string is materialized and appended.
- File reading and post-extraction text/Markdown processing run on a tracked
  blocking worker with cooperative cancellation and deadline checkpoints.
  IronFlow parses the input once with `lopdf`, releases the original byte
  buffer, and extracts one page at a time. Document loading and each bounded
  content/CMap decoding are synchronous library calls and cannot be interrupted
  midway; IronFlow checks cancellation before/after them, every 64 KiB of a
  ToUnicode scan, and between operations, reference hops, text operands,
  glyph-string pieces and replacement-string validation chunks. The CMap call
  that cannot be interrupted is bounded by the entries admitted above. Page
  content and font streams use the smaller of the remaining output budget and
  the shared decompressed-stream limit. This is not a peak-memory ceiling:
  parsed operations and temporary glyph-decoding strings coexist with the
  retained document and result, and the font maps of every font used so far
  are kept until the call ends. `lopdf` 0.47 holds roughly 130 bytes for each
  admitted entry, about 130 MiB at the default entry limit. Task and run
  admission remain occupied until the physical worker stops.

## Example

```lua
local flow = Flow.new("read_pdf")

flow:step("extract", nodes.extract_pdf({
    path = "${ctx.file_path}",
    format = "text",
    output_key = "pdf_text",
    metadata_key = "pdf_meta"
}))

flow:step("done", nodes.log({
    message = "Extracted ${ctx.pdf_meta.pages} PDF pages"
})):depends_on("extract")

return flow
```
