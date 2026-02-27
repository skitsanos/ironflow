local flow = Flow.new("image_flip_demo")

flow:step("flip", nodes.image_flip({
    path = "data/samples/markdown-aware-chunking.jpeg",
    direction = "vertical",
    output_path = "data/samples/sample_flip.png",
    output_key = "flipped"
}))

flow:step("log", nodes.log({
    message = "Flipped image: ${ctx.flipped}"
})):depends_on("flip")

return flow

