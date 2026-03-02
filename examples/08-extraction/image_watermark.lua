local flow = Flow.new("image_watermark_demo")

flow:step("watermark", nodes.image_watermark({
    path = "data/samples/semantic-chunking.jpeg",
    output_path = "/tmp/semantic-chunking-watermark.png",
    text = "IRONFLOW",
    position = "bottom-right",
    opacity = 0.45,
    output_key = "watermarked_image",
}))

flow:step("log", nodes.log({
    message = "Watermarked image: ${ctx.watermarked_image_path}",
    level = "info",
})):depends_on("watermark")

return flow
