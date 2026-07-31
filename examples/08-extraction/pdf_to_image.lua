-- Requirements: native Pdfium must be installed system-wide, available in the
-- working directory, or selected with PDFIUM_LIB_PATH.
-- Effects: rendered images are stored under IRONFLOW_ARTIFACT_DIR; context
-- receives page metadata plus content-addressed descriptors. Artifacts are not
-- automatically pruned.
local flow = Flow.new("pdf_to_image_demo")

-- Render page 1 of a PDF to PNG at 150 DPI
flow:step("render", nodes.pdf_to_image({
    path = "${ctx._flow_dir}/../fixtures/ironflow-sample.pdf",
    pages = "1",
    format = "png",
    dpi = 150,
    output_key = "images"
}))

-- ctx.images[1].artifact can be passed directly to image nodes.
flow:step("summarize", function(ctx)
    return { rendered_count = #(ctx.images or {}) }
end):depends_on("render")

flow:step("info", nodes.log({
    message = "Rendered ${ctx.rendered_count} of ${ctx.page_count} PDF pages"
})):depends_on("summarize")

return flow
