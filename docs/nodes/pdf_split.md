# `pdf_split`

Split a PDF into individual pages or page ranges, saving each as a separate PDF file.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | one of `path` or `source_key` | — | File path to the PDF; supports `${ctx.*}` interpolation. |
| `source_key` | string | one of `path` or `source_key` | — | Context key whose value is the file path (must be a string). |
| `output_dir` | string | yes | — | Directory for output files; supports `${ctx.*}` interpolation. |
| `pages` | string | no | `"all"` | Page specification: `"all"`, a single page `"3"`, a range `"1-5"`, or a combination `"1-3,7,9-11"`. Pages are 1-based. |
| `output_key` | string | no | `"pdf_split"` | Context key prefix for output values. |

> Providing both `path` and `source_key` is an error.

## Context Output

- `<output_key>_files` (default `pdf_split_files`) — array of file paths for the split PDF pages.
- `<output_key>_page_count` (default `pdf_split_page_count`) — number of pages extracted.
- `<output_key>_success` (default `pdf_split_success`) — `true` on success.

## Example

```lua
local flow = Flow.new("split_pdf")

flow:step("split", nodes.pdf_split({
    path = "/data/document.pdf",
    output_dir = "/data/pages",
    pages = "1-3,5"
}))

flow:step("done", nodes.log({
    message = "Split into ${ctx.pdf_split_page_count} files"
})):depends_on("split")

return flow
```
