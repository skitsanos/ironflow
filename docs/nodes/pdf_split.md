# `pdf_split`

Select PDF pages using individual numbers or ranges. Write one file per page by
default, or opt into multipage slices with `pages_per_file`.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | one of `path` or `source_key` | — | File path to the PDF; supports `${ctx.key}` interpolation. |
| `source_key` | string | one of `path` or `source_key` | — | Context key containing a file path, artifact URI, or artifact descriptor. |
| `output_dir` | string | yes | — | Directory for output files; supports `${ctx.key}` interpolation. |
| `pages` | string | no | `"all"` | Page specification: `"all"`, a single page `"3"`, a range `"1-5"`, or a combination `"1-3,7,9-11"`. Pages are 1-based; supports `${ctx.key}` interpolation before parsing. |
| `pages_per_file` | integer | no | `1` | Maximum selected pages in each output file, from `1` to `IRONFLOW_MAX_PDF_SPLIT_PAGES` (default `1000`). Values greater than `1` enable grouped output. Strings, null, fractions, zero, and negative values are errors. |
| `output_key` | string | no | `"pdf_split"` | Context key prefix for output values. |

> Providing both `path` and `source_key` is an error.
> Artifact inputs are opened and SHA-256 verified inside the tracked blocking worker; PDF parsing consumes that same rewound handle rather than a resolved store pathname.

For example, `pages = "${ctx.range}"` with `range = "31-60"` creates 30 one-page
files by default. Add `pages_per_file = 30` to produce one 30-page file instead.
Missing or null context values resolve to empty text
and fail the existing page-specification parser. Invalid resolved selectors fail
before creating the output directory. Literal selectors retain their behavior,
including ordering and repeated selections in the default single-page mode.

## Grouped output

With `pages_per_file > 1`, the node selects pages first and groups that sequence
without sorting it. `pages = "all"` for a 243-page PDF with `pages_per_file = 30`
creates nine files: eight with 30 pages and the last with three. A selector of
`"5,1-3,9"` with `pages_per_file = 3` creates groups `[5,1,2]` and `[3,9]`.
Repeated selections, including overlapping ranges, are rejected before creating
the output directory. Multiple selected entries referencing the same source
page object are also rejected in grouped mode.

Grouped filenames are `<stem>_part_001.pdf`, `<stem>_part_002.pdf`, and so on,
with a minimum three-digit part number. Part numbers describe selection order,
not original page ranges; use the output metadata for exact source page numbers.
The stem is the source filename without its extension for path inputs, or the
SHA-256 digest for artifact inputs, matching legacy stem selection.
The last group may be smaller than `pages_per_file`, including one page.

Each group is flushed and synced to a temporary sibling file, then atomically
published without overwriting. Existing files, final symlinks (including dangling
ones), and nonregular destinations are refused. Destination checks occur per
group, with collision protection at publication as well. Directory aliases are
resolved using the shared rooted writer; on Unix the resolved directory is
pinned by an open handle. Other platforms use the writer's checked-path fallback.

This is **per-file atomicity, not an all-or-nothing batch**: completed groups
remain after a later collision, parse/write failure, or cancellation. Unfinished
staging files are removed on normal error/cancellation unwinding, not guaranteed
after a process crash. No success output or parts metadata is returned on node
failure. Use a fresh output directory for each attempt; an automatic node retry
does not resume and will encounter any files already published.

Omitting `pages_per_file` or setting it to `1` preserves legacy filenames
(`<stem>_<source-page>.pdf`), repeated file paths for duplicate selections,
existing-file overwrite behavior, direct non-atomic writes, and the three
original output keys. Grouped collision and staging rules do not alter this path.

## Limits and cancellation

Page selection is bounded by `IRONFLOW_MAX_PDF_SPLIT_PAGES` (default `1000`).
`"all"` and explicit or repeated ranges are rejected before the selector
allocates more indices than that ceiling. The source must be a regular file and
is capped by `IRONFLOW_MAX_PDF_BYTES` (default 100 MiB) before and during reads;
post-open growth cannot cross the cap. Loading, object traversal, and writes run
on a tracked blocking worker with cancellation checkpoints around opaque
`lopdf` operations and between pages/groups. The source is loaded once; grouped
output does not write or reparse intermediate one-page files. `lopdf` retains
the bounded source document and one page/group's reachable cloned object graph
while producing that output, so the source byte cap is not an exact RSS ceiling.
A later failure
does not roll back page files already written to `output_dir`.

Before writing any pages, [shared PDF loading](../PDF_LOADING.md) enforces
`IRONFLOW_MAX_PDF_DECOMPRESSED_STREAM_BYTES` (64 MiB) per object/cross-reference
stream and rejects more than `IRONFLOW_MAX_PDF_OBJECTS` (250000) loaded objects
after parsing. Strict parsing and bounded recovery checks prevent oversized
streams from silently producing partial documents.

Grouped output additionally applies `IRONFLOW_MAX_PDF_OBJECTS` to each retained
group graph, counting shared resources once plus the new page tree and catalog.
This check runs during graph collection, before cloning beyond that count.
`IRONFLOW_MAX_PDF_BYTES` also caps each serialized grouped file during staged
writes. It is not a cumulative output-directory byte limit, and grouping is by
page count only: a too-large group fails rather than being automatically divided
again. There is no hardcoded OCR-provider page limit.

## Page isolation and fidelity

Each output contains its selected pages and their reachable dependencies, without
copying the source page tree, omitted pages, or their unique resources. Before
reparenting, the node materializes the nearest inherited `Resources`, `MediaBox`,
`CropBox`, and `Rotate` values. Explicit page values take precedence; null values
are treated as absent. Broken or cyclic parent chains fail before that page/group is
written. Graph collection and remapping include cancellation checkpoints.

References to omitted pages, source page-tree nodes, or the source catalog become
PDF nulls, so a cross-page annotation cannot pull an excluded page back into the
file. Links between pages retained in the same group and annotation backlinks
are remapped normally. Shared fonts and images are retained once per group.
Document-level bookmarks, signatures, and source catalog features are not preserved. This is
page extraction, not content redaction: retained shared resources, metadata, or
streams may contain unused or hidden data and are not sanitized.

## Context Output

- `<output_key>_files` (default `pdf_split_files`) — array of output PDF paths in selection/group order.
- `<output_key>_page_count` (default `pdf_split_page_count`) — total selected pages, not file count.
- `<output_key>_success` (default `pdf_split_success`) — `true` on success.
- `<output_key>_parts` (default `pdf_split_parts`) — grouped mode only: one object per file, containing `path`, ordered 1-based source `pages`, and `page_count`.

For example, selecting `"5,1-3,9"` in groups of three returns:

```json
[
  {"path": "/data/parts/document_part_001.pdf", "pages": [5, 1, 2], "page_count": 3},
  {"path": "/data/parts/document_part_002.pdf", "pages": [3, 9], "page_count": 2}
]
```

## Example

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

For direct provider-sized slices, use the runnable
[`pdf_split_grouped.lua`](../../examples/08-extraction/pdf_split_grouped.lua)
example. It defaults to the bundled three-page fixture and groups of 30. To
test another PDF, run from the repository root:

```bash
ironflow run examples/08-extraction/pdf_split_grouped.lua \
  --context '{"input_path":"/absolute/path/document.pdf","pages_spec":"all"}'
```

The example logs source-page metadata and uses a fresh temporary output
directory per run. The same flow works through `serve` with `file` and `context`
in `POST /flows/run` when its example directory is configured under `flows_dir`.
