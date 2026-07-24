-- Requirements: native Pdfium must be installed system-wide, available in the
-- working directory, or selected with PDFIUM_LIB_PATH.
-- Effects: the thumbnail is returned in workflow context; no file is written.
local flow = Flow.new("pdf_thumbnail_demo")

-- Render the first page as a thumbnail image.
flow:step("thumb", nodes.pdf_thumbnail({
    path = "${ctx._flow_dir}/../fixtures/ironflow-sample.pdf",
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
