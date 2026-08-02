# `pdf_split`

Split a PDF into individual pages or page ranges, saving each as a separate PDF file.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | one of `path` or `source_key` | — | File path to the PDF; supports `${ctx.key}` interpolation. |
| `source_key` | string | one of `path` or `source_key` | — | Context key containing a file path, artifact URI, or artifact descriptor. |
| `output_dir` | string | yes | — | Directory for output files; supports `${ctx.key}` interpolation. |
| `pages` | string | no | `"all"` | Page specification: `"all"`, a single page `"3"`, a range `"1-5"`, or a combination `"1-3,7,9-11"`. Pages are 1-based. |
| `output_key` | string | no | `"pdf_split"` | Context key prefix for output values. |

> Providing both `path` and `source_key` is an error.
> Artifact inputs are opened and SHA-256 verified inside the tracked blocking worker; PDF parsing consumes that same rewound handle rather than a resolved store pathname.

Page selection is bounded by `IRONFLOW_MAX_PDF_SPLIT_PAGES` (default `1000`).
`"all"` and explicit or repeated ranges are rejected before the selector
allocates more indices than that ceiling. The source must be a regular file and
is capped by `IRONFLOW_MAX_PDF_BYTES` (default 100 MiB) before and during reads;
post-open growth cannot cross the cap. Loading, object traversal, and writes run
on a tracked blocking worker with cancellation checkpoints around opaque
`lopdf` operations and between pages. `lopdf` still retains the bounded source
document and one selected page's reachable cloned object graph while producing
that page, so the source byte cap is not an exact RSS ceiling. A later failure
does not roll back page files already written to `output_dir`.

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
