local flow = Flow.new("image_rotate_demo")

flow:step("rotate", nodes.image_rotate({
    path = "data/samples/sample_front.png",
    angle = 90,
    output_path = "output/sample_front_rotated.png",
    output_key = "rotated"
}))

flow:step("log", nodes.log({
    message = "Rotated image size: ${ctx.rotated_width}x${ctx.rotated_height}"
})):depends_on("rotate")

return flow

