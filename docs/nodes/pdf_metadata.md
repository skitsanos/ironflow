# `pdf_metadata`

Read a PDF's page count and document information fields (title, author, dates and so on).

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | One of `path` or `source_key` | — | PDF file path or `artifact://` URI; supports `${ctx.key}` interpolation. |
| `source_key` | string | One of `path` or `source_key` | — | Context key holding a path, artifact URI or artifact descriptor; used as stored, without interpolation. |
| `output_key` | string | No | `"metadata"` | Context key for the metadata object. |

`path` and `source_key` are mutually exclusive.

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}` | object | Page count and the document information fields that are present. |

## Output Shape

```json
{
  "pages": 3,
  "title": "Quarterly Report",
  "author": "Finance Team",
  "creator": "Microsoft Word",
  "producer": "macOS Quartz PDFContext",
  "created": "D:20260115093000Z",
  "modified": "D:20260116101500Z"
}
```

- `pages` is always present. `title`, `author`, `subject`, `keywords`, `creator`, `producer`, `created` and `modified` appear only when set and non-empty.
- `created` and `modified` are the raw PDF date strings (`D:YYYYMMDDHHmmSS...`).

## Behavior

- Text fields are decoded from PDFDocEncoding, UTF-16 or UTF-8, trimmed, and stripped of control characters and language-tag escapes; invalid sequences become U+FFFD.
- A field that is missing, not a string, or empty after decoding is omitted rather than set to `null`. Use a Lua step to supply a default.
- A source that is not a regular file, is a symlink, or is not a valid PDF fails the step.

## Limits

| Variable | Default | Effect |
|----------|---------|--------|
| `IRONFLOW_MAX_PDF_BYTES` | 100 MiB | Maximum source size. |
| `IRONFLOW_MAX_PDF_DECOMPRESSED_STREAM_BYTES` | 64 MiB | Maximum decompressed size of one PDF stream. |
| `IRONFLOW_MAX_PDF_OBJECTS` | `250000` | Maximum objects in the source. |

See [PDF loading](../PDF_LOADING.md) for how these limits apply.

## Examples

### Read metadata with a default

```lua
local flow = Flow.new("pdf_metadata_demo")

flow:step("meta", nodes.pdf_metadata({
    path = "examples/fixtures/ironflow-sample.pdf",
    output_key = "pdf_meta"
}))

-- Interpolation does not evaluate fallback expressions, so normalize the
-- optional field in an explicit workflow step.
flow:step("metadata_defaults", function(ctx)
    return {
        pdf_creator = ctx.pdf_meta.creator or "unknown"
    }
end):depends_on("meta")

flow:step("log", nodes.log({
    message = "PDF has ${ctx.pdf_meta.pages} page(s), produced by ${ctx.pdf_creator}"
})):depends_on("metadata_defaults")

return flow
```
