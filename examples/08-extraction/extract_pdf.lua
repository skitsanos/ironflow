local flow = Flow.new("extract_pdf_demo")

-- Extract text from a PDF document
flow:step("extract_text", nodes.extract_pdf({
    path = "data/samples/Bill26022026_121916AM_8000951511_fc72420d-72e1-460b-b714-8a7388ea90d4_.pdf",
    format = "text",
    output_key = "text_content",
    metadata_key = "metadata"
}))

flow:step("show_meta", nodes.log({
    message = "PDF Metadata: ${ctx.metadata}"
})):depends_on("extract_text")

flow:step("show_text", nodes.log({
    message = "PDF Text: ${ctx.text_content}"
})):depends_on("extract_text")

-- Also extract as markdown
flow:step("extract_md", nodes.extract_pdf({
    path = "data/samples/Bill26022026_121916AM_8000951511_fc72420d-72e1-460b-b714-8a7388ea90d4_.pdf",
    format = "markdown",
    output_key = "md_content"
})):depends_on("extract_text")

flow:step("show_md", nodes.log({
    message = "PDF Markdown: ${ctx.md_content}"
})):depends_on("extract_md")

return flow
