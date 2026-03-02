# `pdf_merge`

Merge multiple PDF files into a single PDF document.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `files` | array | yes | — | Array of file paths to merge; supports `${ctx.*}` interpolation on each entry. |
| `output_path` | string | yes | — | File path for the merged output PDF; supports `${ctx.*}` interpolation. |
| `output_key` | string | no | `"pdf_merge"` | Context key prefix for output values. |

## Context Output

- `<output_key>_path` (default `pdf_merge_path`) — path to the merged PDF file.
- `<output_key>_page_count` (default `pdf_merge_page_count`) — total number of pages in the merged document.
- `<output_key>_success` (default `pdf_merge_success`) — `true` on success.

## Example

```lua
local flow = Flow.new("merge_pdfs")

flow:step("merge", nodes.pdf_merge({
    files = {
        "/data/report_part1.pdf",
        "/data/report_part2.pdf",
        "/data/report_part3.pdf"
    },
    output_path = "/data/full_report.pdf"
}))

flow:step("done", nodes.log({
    message = "Merged ${ctx.pdf_merge_page_count} pages into ${ctx.pdf_merge_path}"
})):depends_on("merge")

return flow
```
