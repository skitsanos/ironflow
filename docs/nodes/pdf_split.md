# `pdf_split`

Select PDF pages by number or range and write one file per page, or groups of pages with `pages_per_file`.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | One of `path` or `source_key` | — | PDF file path or `artifact://` URI; supports `${ctx.key}` interpolation. |
| `source_key` | string | One of `path` or `source_key` | — | Context key holding a path, artifact URI or artifact descriptor; used as stored, without interpolation. |
| `output_dir` | string | Yes | — | Directory for output files; supports `${ctx.key}` interpolation. |
| `pages` | string | No | `"all"` | `"all"`, `"3"`, `"1-5"`, or a list such as `"1-3,7,9-11"`; 1-based, interpolated before parsing. |
| `pages_per_file` | integer | No | `1` | Pages per output file, from 1 to `IRONFLOW_MAX_PDF_SPLIT_PAGES`; values above 1 enable grouped output. |
| `output_key` | string | No | `"pdf_split"` | Prefix for the context keys written by this node. |

`path` and `source_key` are mutually exclusive.

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}_files` | array | Output file paths, in selection order. |
| `{output_key}_page_count` | number | Total selected pages (not the file count). |
| `{output_key}_success` | boolean | `true` on success. |
| `{output_key}_parts` | array | Grouped mode only: one object per file. |

## Output Shape

`pages = "5,1-3,9"` with `pages_per_file = 3` gives this `{output_key}_parts`:

```json
[
  {"path": "/data/parts/document_part_001.pdf", "pages": [5, 1, 2], "page_count": 3},
  {"path": "/data/parts/document_part_002.pdf", "pages": [3, 9], "page_count": 2}
]
```

`pages` lists the 1-based source page numbers in each file.

## Behavior

- Single-page mode writes `<stem>_<page>.pdf` and replaces an existing file with that name; repeated selections write the same file again.
- Grouped mode writes `<stem>_part_001.pdf`, `<stem>_part_002.pdf`, ... in selection order without sorting; the last part may be smaller. An existing file with that name fails the step.
- `<stem>` is the source file name without extension, or the SHA-256 digest for artifact inputs.
- A page number of 0, a page past the end, a reversed range, or an empty selection fails the step before `output_dir` is created. In grouped mode a repeated page also fails.
- A destination that is a symlink or not a regular file fails the step.
- Files already written stay in place when a later page or group fails; use a fresh `output_dir` per attempt.
- Each output keeps only its pages and the resources they use. Bookmarks, signatures and document-level features are not kept, and retained resources are not sanitized.
- Missing directories are created as described in [Created directories](write_file.md#created-directories).

## Limits

| Variable | Default | Effect |
|----------|---------|--------|
| `IRONFLOW_MAX_PDF_SPLIT_PAGES` | `1000` | Maximum selected pages, and the maximum `pages_per_file`. |
| `IRONFLOW_MAX_PDF_BYTES` | 100 MiB | Maximum source size, and maximum size of each output file. |
| `IRONFLOW_MAX_PDF_DECOMPRESSED_STREAM_BYTES` | 64 MiB | Maximum decompressed size of one PDF stream. |
| `IRONFLOW_MAX_PDF_OBJECTS` | `250000` | Maximum objects in the source, and in each grouped output. |

See [PDF loading](../PDF_LOADING.md) for how the source limits apply.

## Examples

### Split selected pages

```lua
local flow = Flow.new("split_pdf")

flow:step("select", function(ctx)
    return { range = "1-3,5" }
end)

flow:step("split", nodes.pdf_split({
    path = "/data/document.pdf",
    output_dir = "/data/pages",
    pages = "${ctx.range}"
})):depends_on("select")

flow:step("done", nodes.log({
    message = "Split into ${ctx.pdf_split_page_count} files"
})):depends_on("split")

return flow
```

### Grouped slices

The runnable [`pdf_split_grouped.lua`](../../examples/08-extraction/pdf_split_grouped.lua) example splits a PDF into groups of 30 pages:

```bash
ironflow run examples/08-extraction/pdf_split_grouped.lua \
  --context '{"input_path":"/absolute/path/document.pdf","pages_spec":"all"}'
```
