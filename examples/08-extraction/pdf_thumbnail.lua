local flow = Flow.new("pdf_thumbnail_demo")

-- Render the first page as a thumbnail image.
flow:step("thumb", nodes.pdf_thumbnail({
    path = "data/samples/Bill26022026_121916AM_8000951511_fc72420d-72e1-460b-b714-8a7388ea90d4_.pdf",
    page = 1,
    format = "png",
    size = 320,
    dpi = 150,
    output_key = "preview"
}))

flow:step("log", nodes.log({
    message = "Thumb: ${ctx.preview.width}x${ctx.preview.height}"
})):depends_on("thumb")

return flow
