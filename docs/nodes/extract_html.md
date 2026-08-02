# `extract_html`

Extract text and metadata from an HTML file.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | one of `path` or `source_key` | — | File path to the HTML file; supports `${ctx.key}` interpolation. |
| `source_key` | string | one of `path` or `source_key` | — | Context key containing a file path, artifact URI, or artifact descriptor. |
| `format` | string | no | `"text"` | Output format: `"text"` sanitizes the HTML, converts it through the HTML-to-Markdown converter, and trims each output line; converter-produced Markdown markers can remain. `"markdown"` converts the original HTML directly to Markdown. |
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
- File reading, structural inspection, metadata scanning, and result
  serialization run on a tracked blocking worker and check cancellation and
  the step/run deadline cooperatively. The third-party HTML sanitizer and
  HTML-to-Markdown converter cannot be interrupted inside one call; IronFlow
  checks immediately before and after those calls. Their returned strings are
  materialized before the extraction-output limit can inspect them, so that
  logical result limit does not cap the libraries' transient peak allocation.
  Task and run admission remain occupied until the physical worker stops.

## Example

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
