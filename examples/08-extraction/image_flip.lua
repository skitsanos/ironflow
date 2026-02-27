local flow = Flow.new("image_flip_demo")

flow:step("flip", nodes.image_flip({
    path = "data/samples/sample_front.png",
    direction = "vertical",
    output_path = "output/sample_front_flip.png",
    output_key = "flipped"
}))

flow:step("log", nodes.log({
    message = "Flipped image: ${ctx.flipped}"
})):depends_on("flip")

return flow

