-- Run from the repository root; both outputs are created without external converters.
local flow = Flow.new("markdown_multilingual")

flow:step("prepare", function(ctx)
    local function default(value, fallback)
        if value == nil then return fallback end
        return value
    end
    return {
        markdown_path = default(ctx.markdown_path, "examples/fixtures/markdown-multilingual.md"),
        pdf_path = default(ctx.pdf_path, "data/output/markdown/multilingual.pdf"),
        docx_path = default(ctx.docx_path, "data/output/markdown/multilingual.docx"),
        language = default(ctx.language, "und"),
        direction = default(ctx.direction, "auto")
    }
end)

flow:step("source", nodes.read_file({
    path = "${ctx.markdown_path}", output_key = "markdown"
})):depends_on("prepare")

flow:step("pdf", nodes.markdown_to_pdf({
    source_key = "markdown_content", output_path = "${ctx.pdf_path}", output_key = "pdf",
    language = "${ctx.language}", direction = "${ctx.direction}",
    title = "Multilingual Document"
})):depends_on("source")

flow:step("docx", nodes.markdown_to_docx({
    source_key = "markdown_content", output_path = "${ctx.docx_path}", output_key = "docx",
    language = "${ctx.language}", direction = "${ctx.direction}",
    title = "Multilingual Document"
})):depends_on("source")

flow:step("extract", nodes.extract_pdf({
    path = "${ctx.pdf_path}", output_key = "pdf_text", metadata_key = "pdf_text_metadata"
})):depends_on("pdf")

return flow
