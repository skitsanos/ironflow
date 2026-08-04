-- Supported inputs: BMP, Farbfeld, GIF, HDR, ICO, JPEG, PNG, PNM, QOI, TGA,
-- TIFF, and WebP.
local flow = Flow.new("image_metadata_demo")

flow:step("meta", nodes.image_metadata({
    path = "${ctx._flow_dir}/../fixtures/ironflow-sample.png",
    output_key = "img"
}))

flow:step("log", nodes.log({
    message = "Image: ${ctx.img_width}x${ctx.img_height} format=${ctx.img_format} color=${ctx.img_color_type}"
})):depends_on("meta")

return flow
