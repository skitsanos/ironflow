local flow = Flow.new("pdf_split_example")

flow:step("split", nodes.pdf_split({
    path = "examples/08-extraction/sample.pdf",
    output_dir = "/tmp/pdf_pages",
    pages = "1-3"
}))

flow:step("log_result", nodes.log({
    message = "Split into ${ctx.pdf_split_page_count} pages: ${ctx.pdf_split_files}"
})):depends_on("split")

return flow
