local flow = Flow.new("image_crop_demo")

flow:step("crop", nodes.image_crop({
    path = "data/samples/markdown-aware-chunking.jpeg",
    output_path = "data/samples/markdown-aware-chunking_cropped.png",
    x = 10,
    y = 8,
    width = 400,
    height = 300,
    format = "png",
    output_key = "cropped"
}))

flow:step("log", nodes.log({
    message = "Cropped image written to ${ctx.cropped} (${ctx.cropped_width}x${ctx.cropped_height})"
})):depends_on("crop")

return flow
