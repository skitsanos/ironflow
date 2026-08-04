# `extract_pdf`

Extract text and metadata from a PDF document.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | one of `path` or `source_key` | — | File path to the PDF; supports `${ctx.key}` interpolation. |
| `source_key` | string | one of `path` or `source_key` | — | Context key containing a file path, artifact URI, or artifact descriptor. |
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

## Resource and cancellation contract

- The input must be a regular file. `IRONFLOW_MAX_PDF_BYTES` (default
  `104857600`, 100 MiB) bounds both its declared size and actual bytes read. On
  Unix, IronFlow also refuses to follow a final path-component symlink; other
  platforms enforce the opened-handle regular-file check.
- `IRONFLOW_MAX_PDF_EXTRACT_PAGES` (default `1000`) rejects the document after
  its page tree is parsed but before text extraction begins.
- `IRONFLOW_MAX_EXTRACT_ITEMS` (default `250000`) is cumulative across PDF
  pages, supported metadata fields that are present, and extracted text lines.
- `IRONFLOW_MAX_EXTRACT_OUTPUT_BYTES` (default `52428800`, 50 MiB) bounds the
  extracted text before it is appended and the complete serialized
  `NodeOutput`, including content and requested metadata. The remaining text
  budget also bounds each page's decompressed content and font-mapping streams.
  This is not the later `IRONFLOW_MAX_TASK_OUTPUT_BYTES` persistence limit.
- File reading and post-extraction text/Markdown processing run on a tracked
  blocking worker with cooperative cancellation and deadline checkpoints.
  IronFlow parses the input once with `lopdf`, releases the original byte
  buffer, and extracts one page at a time. Document loading and each bounded
  page extraction are synchronous library calls and cannot be interrupted
  midway; IronFlow checks cancellation immediately before and after them.
  Previously extracted text plus the next page's decompressed content cannot
  exceed the configured extraction-output budget. Task and run admission
  remain occupied until the physical worker stops.

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
