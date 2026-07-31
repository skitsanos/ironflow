# `pdf_metadata`

Extract metadata from a PDF file (document info dictionary + page count).

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | one of `path` or `source_key` | — | PDF file path |
| `source_key` | string | one of `path` or `source_key` | — | Context key containing a file path, artifact URI, or artifact descriptor |
| `output_key` | string | no | `"metadata"` | Prefix for output key |

> Providing both `path` and `source_key` is an error.

## Resource contract

The source must be a regular file and is capped by
`IRONFLOW_MAX_PDF_BYTES` (default 100 MiB) before and while it is read; a file
that grows after the initial metadata check cannot cross the limit. On Unix the
final path component may not be a symlink. Loading and metadata traversal run on
a tracked blocking worker with cancellation checkpoints around and during input
reads. `lopdf` still constructs its document object model in memory, so the byte
ceiling bounds raw input rather than promising that parser RSS equals file size.

## Context Output

- `<output_key>` — object containing metadata:
  - `pages` — page count
  - `title`, `author`, `subject`, `keywords`, `creator`, `producer`, `created`, `modified` when present

## Example

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
