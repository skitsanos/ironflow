-- Effects: retains one UUID-scoped JPEG under TMPDIR, TMP, TEMP, or `.`.
local flow = Flow.new("image_convert_demo")
local temp_root = env("TMPDIR")
if temp_root == nil or temp_root == "" then temp_root = env("TMP") end
if temp_root == nil or temp_root == "" then temp_root = env("TEMP") end
if temp_root == nil or temp_root == "" then temp_root = "." end
local output_path = temp_root .. "/ironflow-image-convert-" .. uuid4() .. ".jpg"

flow:step("convert", nodes.image_convert({
    path = "${ctx._flow_dir}/../fixtures/ironflow-sample.png",
    output_path = output_path,
    quality = 80,
    output_key = "converted_image",
}))

flow:step("log", nodes.log({
    message = "Converted image saved to ${ctx.converted_image_path} as ${ctx.converted_image_format}",
    level = "info",
})):depends_on("convert")

return flow
