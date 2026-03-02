local flow = Flow.new("image_convert_demo")

flow:step("convert", nodes.image_convert({
    path = "data/samples/semantic-chunking.jpeg",
    output_path = "/tmp/semantic-chunking-converted.jpg",
    quality = 80,
    output_key = "converted_image",
}))

flow:step("log", nodes.log({
    message = "Converted image saved to ${ctx.converted_image_path} as ${ctx.converted_image_format}",
    level = "info",
})):depends_on("convert")

return flow
