-- Requirements: native Pdfium must be installed system-wide, available in the
-- working directory, or selected with PDFIUM_LIB_PATH.
-- Effects: rendered images are returned in workflow context; no files are written.
local flow = Flow.new("pdf_to_image_demo")

-- Render page 1 of a PDF to PNG at 150 DPI
flow:step("render", nodes.pdf_to_image({
    path = "${ctx._flow_dir}/../fixtures/ironflow-sample.pdf",
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
