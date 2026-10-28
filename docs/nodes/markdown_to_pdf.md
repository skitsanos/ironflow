# `markdown_to_pdf`

Generate a searchable, paginated PDF from CommonMark/GFM Markdown, with embedded
fonts, tables and explicitly bound raster images. No Pdfium or external renderer
is required to generate the file.

See [Markdown Documents](../MARKDOWN_DOCUMENTS.md) for the complete parameter
table, supported syntax, fixed resource limits, cancellation and durability
contract. Bundled fonts and bidi-aware layout support Ukrainian, Hebrew, Arabic
and mixed-direction text; Chinese, Japanese and Korean need a build with the
`cjk-fonts` feature (`-full` binaries, the container image) or a CJK font in
`IRONFLOW_FONT_DIR`, and fail with a clear error otherwise. `direction` defaults
to `auto`; `language` defaults to `und`. Missing glyphs fail rather than silently
disappearing. Logical ActualText spans preserve Unicode for compatible readers;
IronFlow's `extract_pdf` consumes these spans for the tested multilingual round
trip. Untagged PDFs and other readers still have font/reading-order limitations.
Links include their URLs
as text, not clickable annotations. No PDF/A or PDF/UA compliance is claimed.

```lua
local flow = Flow.new("markdown_pdf")

flow:step("render", nodes.markdown_to_pdf({
    source_key = "report_markdown",
    output_path = "output/report.pdf",
    output_key = "pdf",
    page_size = "a4",
    margin = 54,
    font_size = 11,
    title = "Quarterly Report"
}))

return flow
```

Outputs are `pdf` (path), `pdf_success` and `pdf_metadata`, including a known
`pages` count. Existing output files require explicit `overwrite = true`.
Use [markdown_to_docx](markdown_to_docx.md) for an editable Word document.
