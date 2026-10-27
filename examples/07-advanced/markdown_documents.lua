-- Create both documents from the same Markdown without external converters.
-- Run from the repository root:
-- ironflow run examples/07-advanced/markdown_documents.lua
-- Override report_markdown, pdf_path or docx_path through --context.
local flow = Flow.new("markdown_documents")

flow:step("prepare", function(ctx)
    local function default(value, fallback)
        if value == nil then return fallback end
        return value
    end
    return {
        report_markdown = default(ctx.report_markdown, [[# IronFlow Document Report

This report is generated locally from **one Markdown source** into PDF and DOCX.
Both outputs retain the report content, while each format handles layout independently.

## Delivery Status

| Format | Content | Runtime |
| --- | --- | --- |
| PDF | Searchable text and embedded images | IronFlow binary |
| DOCX | Editable paragraphs and tables | IronFlow binary |

1. Prepare the Markdown source.
2. Generate both documents.
   - Review the resulting files.

- [x] Use the same source for both formats
- [x] Embed an explicitly bound local image
- [ ] Review before publishing

## Local Image

![IronFlow clean checkout fixture](sample_image)

## Notes

*Formatting* includes **bold**, `inline code`, and ~~deleted text~~.
See [IronFlow on GitHub](https://github.com/skitsanos/ironflow).

```lua
local format = "pdf"
    -- Code indentation is retained.
```

This is the final paragraph of the report.
]]),
        pdf_path = default(ctx.pdf_path, "data/output/markdown/report.pdf"),
        docx_path = default(ctx.docx_path, "data/output/markdown/report.docx")
    }
end)

flow:step("pdf", nodes.markdown_to_pdf({
    source_key = "report_markdown",
    output_path = "${ctx.pdf_path}",
    output_key = "pdf",
    title = "IronFlow Document Report",
    author = "IronFlow",
    images = { sample_image = "examples/fixtures/ironflow-sample.png" }
})):depends_on("prepare")

flow:step("docx", nodes.markdown_to_docx({
    source_key = "report_markdown",
    output_path = "${ctx.docx_path}",
    output_key = "docx",
    title = "IronFlow Document Report",
    author = "IronFlow",
    images = { sample_image = "examples/fixtures/ironflow-sample.png" }
})):depends_on("prepare")

flow:step("done", nodes.log({
    message = "PDF: ${ctx.pdf}; DOCX: ${ctx.docx}"
})):depends_on("pdf", "docx")

return flow
