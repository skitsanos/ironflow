# `markdown_to_docx`

Generate an editable Word document from CommonMark/GFM Markdown. Headings,
formatted runs, tables, hyperlinks and images are written as native OOXML, not
HTML wrapped in a `.docx` file. No Word, LibreOffice or external converter is
required to generate the document.

See [Markdown Documents](../MARKDOWN_DOCUMENTS.md) for all parameters, supported
syntax, fixed resource limits, cancellation and durability guarantees. Lists and
task items currently use literal markers in indented paragraphs, not native Word
numbering or interactive controls. The consuming viewer supplies fonts and
decides pagination. `direction` (`auto`, `ltr`, `rtl`) and `language` control
multilingual layout. Explicit bidi/run fonts and language metadata support mixed
English/RTL, CJK and Ukrainian content. Fonts are not embedded; install the
corresponding Noto families or provide suitable viewer fallback.
LTR paragraphs/code align left; RTL paragraphs align right, with logical OOXML
alignment for modern readers and physical paragraph-style/compatibility
fallbacks for simpler importers. A default Normal style keeps body text from
inheriting heading formatting. Previewers that discard these properties may
still differ; PDF remains the fixed-layout reference.

```lua
local flow = Flow.new("markdown_docx")

flow:step("render", nodes.markdown_to_docx({
    source_key = "report_markdown",
    output_path = "output/report.docx",
    output_key = "docx",
    title = "Quarterly Report",
    author = "Report Team"
}))

return flow
```

Outputs are `docx` (path), `docx_success` and `docx_metadata`. The metadata's
`pages` is JSON null, not a guessed page count. Existing output files require
explicit `overwrite = true`. Use [markdown_to_pdf](markdown_to_pdf.md) for
fixed-layout searchable PDF output.
