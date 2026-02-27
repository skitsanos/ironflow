local flow = Flow.new("image_to_pdf_demo")

-- Convert existing images into a single PDF.
flow:step("convert", nodes.image_to_pdf({
    sources = {
        { path = "data/samples/sample_front.png" },
        { path = "data/samples/sample_back.png" },
    },
    output_path = "output/generated_report.pdf",
    output_key = "report_pdf"
}))

flow:step("log", nodes.log({
    message = "Created ${ctx.report_pdf} with ${ctx.report_pdf_count} page(s)"
})):depends_on("convert")

return flow
