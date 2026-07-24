-- Effects: retains one UUID-scoped PNG under TMPDIR, TMP, TEMP, or `.`.
local flow = Flow.new("image_crop_demo")
local temp_root = env("TMPDIR")
if temp_root == nil or temp_root == "" then temp_root = env("TMP") end
if temp_root == nil or temp_root == "" then temp_root = env("TEMP") end
if temp_root == nil or temp_root == "" then temp_root = "." end
local output_path = temp_root .. "/ironflow-image-crop-" .. uuid4() .. ".png"

flow:step("crop", nodes.image_crop({
    path = "${ctx._flow_dir}/../fixtures/ironflow-sample.png",
    output_path = output_path,
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
