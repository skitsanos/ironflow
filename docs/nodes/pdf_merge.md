# `pdf_merge`

Merge PDF files or artifacts, in order, into one PDF.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `files` | array | One of `files` or `source_key` | — | Non-empty array of paths, artifact descriptors, or `artifact://sha256/...` URIs; path strings support `${ctx.key}` interpolation. |
| `source_key` | string | One of `files` or `source_key` | — | Context key holding the source array; values are used as stored, without interpolation. |
| `output_path` | string | Yes | — | Merged PDF path; supports `${ctx.key}` interpolation. |
| `output_key` | string | No | `"pdf_merge"` | Prefix for the context keys written by this node. |

`files` and `source_key` are mutually exclusive.

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}_path` | string | Output path after interpolation. |
| `{output_key}_page_count` | number | Total pages in the merged PDF. |
| `{output_key}_success` | boolean | `true` after the file is committed. |

## Behavior

- All pages of each source are appended in source order.
- An empty array, or a source that is not a valid PDF, fails the step.
- The output is published only when complete; any failure leaves an existing `output_path` unchanged.
- A destination that is a symlink or not a regular file fails the step; an existing regular file is replaced.
- Links between merged pages are kept. Bookmarks, signatures and document-level features are not, and content is not sanitized.
- Missing parent directories are created as described in [Created directories](write_file.md#created-directories).

## Limits

| Variable | Default | Effect |
|----------|---------|--------|
| `IRONFLOW_MAX_PDF_MERGE_FILES` | `100` | Maximum number of sources. |
| `IRONFLOW_MAX_PDF_BYTES` | 100 MiB | Maximum size of each source. |
| `IRONFLOW_MAX_PDF_DECOMPRESSED_STREAM_BYTES` | 64 MiB | Maximum decompressed size of one stream in a source. |
| `IRONFLOW_MAX_PDF_OBJECTS` | `250000` | Maximum objects loaded from each source. |
| `IRONFLOW_MAX_PDF_MERGE_BYTES` | 512 MiB | Maximum total input bytes, and maximum output size. |
| `IRONFLOW_MAX_PDF_MERGE_PAGES` | `2000` | Maximum total pages. |
| `IRONFLOW_MAX_PDF_MERGE_OBJECTS` | `250000` | Maximum objects in the merged PDF. |

See [PDF loading](../PDF_LOADING.md) for how the per-source limits apply.

## Examples

### Merge artifacts from earlier steps

```lua
local flow = Flow.new("merge_artifacts")

flow:step("sources", function(ctx)
    return { merge_sources = { ctx.first_artifact, ctx.second_artifact } }
end)

flow:step("merge", nodes.pdf_merge({
    source_key = "merge_sources",
    output_path = "/tmp/full-report.pdf"
})):depends_on("sources")

return flow
```
