-- Effects: retains one UUID-scoped PNG under TMPDIR, TMP, TEMP, or `.`.
local flow = Flow.new("image_resize_demo")
local temp_root = env("TMPDIR")
if temp_root == nil or temp_root == "" then temp_root = env("TMP") end
if temp_root == nil or temp_root == "" then temp_root = env("TEMP") end
if temp_root == nil or temp_root == "" then temp_root = "." end
local output_path = temp_root .. "/ironflow-image-resize-" .. uuid4() .. ".png"

flow:step("resize", nodes.image_resize({
    path = "${ctx._flow_dir}/../fixtures/ironflow-sample.png",
    output_path = output_path,
    width = 140,
    output_key = "resized"
}))

flow:step("log", nodes.log({
    message = "Resized image written to ${ctx.resized} (${ctx.resized_width}x${ctx.resized_height})"
})):depends_on("resize")

return flow
