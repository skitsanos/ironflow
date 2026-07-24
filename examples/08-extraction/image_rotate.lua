-- Effects: retains one UUID-scoped PNG under TMPDIR, TMP, TEMP, or `.`.
local flow = Flow.new("image_rotate_demo")
local temp_root = env("TMPDIR")
if temp_root == nil or temp_root == "" then temp_root = env("TMP") end
if temp_root == nil or temp_root == "" then temp_root = env("TEMP") end
if temp_root == nil or temp_root == "" then temp_root = "." end
local output_path = temp_root .. "/ironflow-image-rotate-" .. uuid4() .. ".png"

flow:step("rotate", nodes.image_rotate({
    path = "${ctx._flow_dir}/../fixtures/ironflow-sample.png",
    angle = 90,
    output_path = output_path,
    output_key = "rotated"
}))

flow:step("log", nodes.log({
    message = "Rotated image size: ${ctx.rotated_width}x${ctx.rotated_height}"
})):depends_on("rotate")

return flow
