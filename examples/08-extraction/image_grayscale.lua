-- Effects: retains one UUID-scoped PNG under TMPDIR, TMP, TEMP, or `.`.
local flow = Flow.new("image_grayscale_demo")
local temp_root = env("TMPDIR")
if temp_root == nil or temp_root == "" then temp_root = env("TMP") end
if temp_root == nil or temp_root == "" then temp_root = env("TEMP") end
if temp_root == nil or temp_root == "" then temp_root = "." end
local output_path = temp_root .. "/ironflow-image-grayscale-" .. uuid4() .. ".png"

flow:step("grayscale", nodes.image_grayscale({
    path = "${ctx._flow_dir}/../fixtures/ironflow-sample.png",
    output_path = output_path,
    output_key = "gray"
}))

flow:step("log", nodes.log({
    message = "Grayscale image: ${ctx.gray_width}x${ctx.gray_height}"
})):depends_on("grayscale")

return flow
