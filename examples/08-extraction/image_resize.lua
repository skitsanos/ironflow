local flow = Flow.new("image_resize_demo")

flow:step("resize", nodes.image_resize({
    path = "data/samples/semantic-chunking.jpeg",
    output_path = "data/samples/sample_small.png",
    width = 140,
    output_key = "resized"
}))

flow:step("log", nodes.log({
    message = "Resized image written to ${ctx.resized} (${ctx.resized_width}x${ctx.resized_height})"
})):depends_on("resize")

return flow
