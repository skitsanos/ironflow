-- Effects: retains one UUID-scoped PNG under TMPDIR, TMP, TEMP, or `.`.
local flow = Flow.new("image_watermark_demo")
local temp_root = env("TMPDIR")
if temp_root == nil or temp_root == "" then temp_root = env("TMP") end
if temp_root == nil or temp_root == "" then temp_root = env("TEMP") end
if temp_root == nil or temp_root == "" then temp_root = "." end
local output_path = temp_root .. "/ironflow-image-watermark-" .. uuid4() .. ".png"

flow:step("watermark", nodes.image_watermark({
    path = "${ctx._flow_dir}/../fixtures/ironflow-sample.png",
    output_path = output_path,
    text = "IRONFLOW",
    position = "bottom-right",
    opacity = 0.45,
    output_key = "watermarked_image",
}))

flow:step("log", nodes.log({
    message = "Watermarked image: ${ctx.watermarked_image_path}",
    level = "info",
})):depends_on("watermark")

return flow
