local flow = Flow.new("extract_word_demo")

-- Extract as text with metadata
flow:step("extract_text", nodes.extract_word({
    path = "data/samples/Ballerina_vs_Java_Comparison_Matrix.docx",
    format = "text",
    output_key = "text_content",
    metadata_key = "metadata"
}))

flow:step("show_meta", nodes.log({
    message = "Metadata: ${ctx.metadata}"
})):depends_on("extract_text")

flow:step("show_text", nodes.log({
    message = "Text content: ${ctx.text_content}"
})):depends_on("extract_text")

-- Extract as markdown (no metadata_key = metadata not emitted again)
flow:step("extract_md", nodes.extract_word({
    path = "data/samples/Ballerina_vs_Java_Comparison_Matrix.docx",
    format = "markdown",
    output_key = "md_content"
})):depends_on("extract_text")

flow:step("show_md", nodes.log({
    message = "Markdown content: ${ctx.md_content}"
})):depends_on("extract_md")

return flow
