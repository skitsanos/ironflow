# `extract_html`

Extract text and metadata from an HTML file.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | one of `path` or `source_key` | — | File path to the HTML file; supports `${ctx.key}` interpolation. |
| `source_key` | string | one of `path` or `source_key` | — | Context key containing a file path, artifact URI, or artifact descriptor. |
| `format` | string | no | `"text"` | Output format: `"text"` reads plain text from the filtered HTML tree; `"markdown"` converts that tree to Markdown, retaining headings, emphasis, lists, tables, links, and code. |
| `output_key` | string | no | `"content"` | Context key where the extracted content is stored. |
| `metadata_key` | string | no | — | If set, HTML metadata is stored under this context key. |

> Providing both `path` and `source_key` is an error.
> Artifact inputs are opened and SHA-256 verified inside the tracked blocking worker; extraction reads that same handle rather than a resolved store pathname.
> The `format` parameter only accepts `"text"` or `"markdown"`; any other value is rejected.
> Present `format`, `output_key`, and `metadata_key` values must be strings; a value of the wrong type is rejected instead of being treated as absent.
> When `metadata_key` is set, it must differ from `output_key`; key collisions are rejected before extraction begins.

## Context Output

- `<output_key>` (default `content`) — the extracted text or Markdown.
- `<metadata_key>` (only when `metadata_key` is set) — an object with available fields: `title`, `description`, `author`, `keywords`, `viewport`, `og:title`, `og:description`, `og:type`, `og:url`.

## Content Filtering

Both formats parse HTML structurally and remove `head`, `title`, `script`,
`style`, `noscript`, and inert `template` subtrees before rendering. Filtering
also applies inside tables and preformatted content. HTML fragments and
malformed markup use the HTML parser's recovery rules, not regular-expression
tag removal. Comments are omitted; entity-encoded literal tags remain text.
Metadata is still extracted separately from the original input, so a document
title remains available under `metadata_key` without appearing in the content.

`text` does not introduce Markdown markers or escaping. Paragraph-level blocks
(`p`, headings, `pre`, `blockquote`, `hr`, `table`, `ul`, `ol`, `dl`, `figure`,
`figcaption`, `form`, `fieldset`, and sectioning elements such as `section`,
`article`, `aside`, `header`, `footer`, `nav`, `main`, and `address`) are
separated from surrounding text by one blank line. List items, table rows,
`dt`/`dd`, and `div` layout blocks are separated by a single newline. `<br>` is
one newline and never widens an existing blank line, so `<br><br>` yields
exactly one blank line and `</p><br><p>` still yields one. Table cells are
separated with spaces and image alternative text is kept. Normal HTML
whitespace is collapsed; preformatted text preserves its internal whitespace.
Leading and trailing newlines are trimmed. This replaces the older text mode's
Markdown-converter output, which could include heading/list markers and
backslash escapes, while keeping its blank-line paragraph separation.

`markdown` keeps the converter's syntax-aware escaping, without blanket
backslash removal. Ordinary prose such as `issue #133` and `C#` is unchanged.
A paragraph containing `# literal heading` becomes `\# literal heading` in
Markdown so it still renders as literal text, not an unintended heading;
`text` returns `# literal heading`. Literal asterisks and backslashes likewise
retain their meaning, while actual HTML emphasis and code retain formatting.
Conversion remains best-effort and does not evaluate CSS visibility, execute
scripts, or fetch external resources. This is content extraction, not a general
HTML/Markdown security sanitizer.

## Resource and cancellation contract

- The input must be a regular file and valid UTF-8. `IRONFLOW_MAX_FILE_BYTES`
  (default `52428800`, 50 MiB) bounds both its declared size and actual bytes
  read. On Unix and Windows, IronFlow also refuses to follow a final
  path-component symlink/reparse point; other platforms enforce the opened-handle regular-file check.
- `IRONFLOW_MAX_EXTRACT_ITEMS` (default `250000`) is a cumulative structural
  budget for the call. For HTML it counts markup items, detected as `<`
  markers while scanning the input.
- `IRONFLOW_MAX_EXTRACT_OUTPUT_BYTES` (default `52428800`, 50 MiB) bounds the
  complete serialized `NodeOutput`, including the configured content key and
  optional metadata object. This is a logical result limit, not a process-RSS
  limit and not the later `IRONFLOW_MAX_TASK_OUTPUT_BYTES` persistence limit.
- File reading, structural inspection, tree filtering, plain-text rendering,
  metadata scanning, and result serialization run on a tracked blocking worker and check cancellation and
  the step/run deadline cooperatively. Third-party HTML parsing and Markdown
  conversion cannot be interrupted internally; IronFlow checks before and
  after them, and during its own filtering and plain-text traversal. The parsed
  tree and converter-produced Markdown are materialized before the
  extraction-output limit can inspect them, so that logical result limit does
  not cap the libraries' transient peak allocation.
  Task and run admission remain occupied until the physical worker stops.

## Example

See [extract_html.lua](../../examples/08-extraction/extract_html.lua) for a
self-contained example that creates one temporary page, extracts both formats
and title metadata, and removes the input. Its style/script/fallback content is
excluded from both outputs.

```lua
local flow = Flow.new("read_html_file")

flow:step("extract", nodes.extract_html({
    path = "/data/page.html",
    format = "markdown",
    output_key = "html_content",
    metadata_key = "html_meta"
}))

flow:step("done", nodes.log({
    message = "Extracted HTML title: ${ctx.html_meta.title}"
})):depends_on("extract")

return flow
```
