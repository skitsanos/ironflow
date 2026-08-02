# `pdf_merge`

Merge bounded PDF path or artifact sources sequentially into one atomically
published PDF.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `files` | array | one of `files` / `source_key` | — | Non-empty array of paths, artifact descriptors, or canonical artifact URIs. Path strings support `${ctx.key}` interpolation. |
| `source_key` | string | one of `files` / `source_key` | — | Context key containing the non-empty source array. Useful when prior nodes produced artifact descriptors. |
| `output_path` | string | yes | — | Merged output path with `${ctx.key}` interpolation. |
| `output_key` | string | no | `"pdf_merge"` | Prefix for output values. |

`files` and `source_key` are mutually exclusive.

## Resource and durability contract

| Environment variable | Default | Boundary |
|----------------------|---------|----------|
| `IRONFLOW_MAX_PDF_MERGE_FILES` | `100` | Source entries admitted before collection |
| `IRONFLOW_MAX_PDF_BYTES` | `104857600` | Bytes in each source PDF |
| `IRONFLOW_MAX_PDF_MERGE_BYTES` | `536870912` | Cumulative input bytes and staged output bytes |
| `IRONFLOW_MAX_PDF_MERGE_PAGES` | `2000` | Cumulative pages |
| `IRONFLOW_MAX_PDF_MERGE_OBJECTS` | `250000` | Retained output graph objects |

Inputs are opened and parsed one at a time on a tracked blocking worker. For
each source, the union of objects reachable from all selected pages is remapped
once, so pages sharing fonts, images, or resources do not clone those objects
per page. Artifact inputs use the same verified file handle used for identity
checking.

The result is written to a sibling staged file, flushed and synchronized, then
atomically committed. Parse, limit, save, and cancellation failures remove the
partial staging file and preserve an existing destination. The output refuses
a final link or non-regular destination.

Artifact inputs require a protected artifact directory visible at the same path
on every eligible worker. A shared mount is not an authentication boundary
against a process running under the same OS identity.

## Context output

- `<output_key>_path` — merged PDF path.
- `<output_key>_page_count` — total page count.
- `<output_key>_success` — `true` after commit.

## Example

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
