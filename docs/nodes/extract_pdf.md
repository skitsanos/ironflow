# `extract_pdf`

Extract the text, and optionally the document metadata, of a PDF file.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | One of `path` or `source_key` | — | Path to the PDF; supports `${ctx.key}` interpolation. |
| `source_key` | string | One of `path` or `source_key` | — | Context key holding a file path, artifact URI, or artifact descriptor; a stored path is used as is, not interpolated. |
| `format` | string | No | `"text"` | `"text"` for raw text, `"markdown"` for paragraph-grouped Markdown. |
| `output_key` | string | No | `"content"` | Context key for the extracted text. |
| `metadata_key` | string | No | — | When set, document metadata is stored under this key; must differ from `output_key`. |

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}` | string | Extracted text or Markdown. |
| `{metadata_key}` | object | Only when `metadata_key` is set: `pages` (number) plus whichever of `title`, `author`, `subject`, `keywords`, `creator`, `producer`, `created`, `modified` the PDF provides. |

## Behavior

- Setting both `path` and `source_key`, neither, a `format` other than `text`/`markdown`, or a non-string value for a string parameter fails the step.
- A requested metadata field that is present but is not a string fails the step; a field that is empty after decoding is omitted. Text fields are decoded as in [`pdf_metadata`](pdf_metadata.md).
- Lines follow text position: text more than half its size off the last baseline starts a new line; superscripts and subscripts stay on their line. `T*`, `'` and `"` always start a new line.
- On one baseline, text that continues where the last string ended is joined, a gap wider than a fifth of the text size becomes one space, and text starting left of that end starts a new line. Inside a `TJ` array, an adjustment below `-100` is a space.
- Fonts without usable widths (no width table, Type3, CMaps other than `Identity-H`) fall back to one line per `BT … ET` block, with strings in a block joined without a separator.
- Marked-content `/ActualText` replaces the enclosed glyphs; the outermost replacement wins and an empty string suppresses the content. Malformed or cyclic properties fail the step.
- Reading order is stream order: columns and table cells are not reordered. Whitespace is best effort, so do not compare output byte for byte. There is no OCR, and text inside Form XObjects is not read.
- All pages are read; use [`pdf_split`](pdf_split.md) first to select pages. `markdown` joins consecutive non-empty lines into paragraphs separated by blank lines.

## Limits

| Variable | Default | Effect |
|----------|---------|--------|
| `IRONFLOW_MAX_PDF_BYTES` | `104857600` (100 MiB) | Maximum input file size. |
| `IRONFLOW_MAX_PDF_EXTRACT_PAGES` | `1000` | Maximum pages in the document. |
| `IRONFLOW_MAX_PDF_EXTRACT_CMAP_ENTRIES` | `1048576` | Maximum ToUnicode mappings across all fonts. |
| `IRONFLOW_MAX_PDF_DECOMPRESSED_STREAM_BYTES` | `67108864` (64 MiB) | Maximum decoded size of one stream. |
| `IRONFLOW_MAX_PDF_OBJECTS` | `250000` | Maximum loaded PDF objects. |
| `IRONFLOW_MAX_EXTRACT_ITEMS` | `250000` | Cumulative pages, fonts, operations, text operands and lines. |
| `IRONFLOW_MAX_EXTRACT_OUTPUT_BYTES` | `52428800` (50 MiB) | Maximum extracted text and serialized output. |

Exceeding a limit fails the step. See [PDF loading](../PDF_LOADING.md) and [`docs/SECURITY.md`](../SECURITY.md) for how they are enforced.

## Examples

### Text and metadata

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
