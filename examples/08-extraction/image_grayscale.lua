local flow = Flow.new("image_grayscale_demo")

flow:step("grayscale", nodes.image_grayscale({
    path = "data/samples/semantic-chunking.jpeg",
    output_path = "data/samples/sample_front_gray.png",
    output_key = "gray"
}))

flow:step("log", nodes.log({
    message = "Grayscale image: ${ctx.gray_width}x${ctx.gray_height}"
})):depends_on("grayscale")

return flow

