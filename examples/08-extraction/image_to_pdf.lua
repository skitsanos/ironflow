local flow = Flow.new("image_to_pdf_demo")

-- Convert existing images into a single PDF.
flow:step("convert", nodes.image_to_pdf({
    sources = {
        { path = "data/samples/semantic-chunking.jpeg" },
        { path = "data/samples/markdown-aware-chunking.jpeg" },
    },
    output_path = "data/samples/generated_book.pdf",
    output_key = "report_pdf"
}))

flow:step("log", nodes.log({
    message = "Created ${ctx.report_pdf} with ${ctx.report_pdf_count} page(s)"
})):depends_on("convert")

return flow
