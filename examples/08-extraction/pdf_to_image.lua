local flow = Flow.new("pdf_to_image_demo")

-- Render page 1 of a PDF to PNG at 150 DPI
flow:step("render", nodes.pdf_to_image({
    path = "data/samples/Bill26022026_121916AM_8000951511_fc72420d-72e1-460b-b714-8a7388ea90d4_.pdf",
    pages = "1",
    format = "png",
    dpi = 150,
    output_key = "images"
}))

-- Show page count and image dimensions
flow:step("info", nodes.log({
    message = "Rendered ${ctx.page_count} page(s)"
})):depends_on("render")

return flow
