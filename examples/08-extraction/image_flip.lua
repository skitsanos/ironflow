-- Effects: retains one UUID-scoped PNG under TMPDIR, TMP, TEMP, or `.`.
local flow = Flow.new("image_flip_demo")
local temp_root = env("TMPDIR")
if temp_root == nil or temp_root == "" then temp_root = env("TMP") end
if temp_root == nil or temp_root == "" then temp_root = env("TEMP") end
if temp_root == nil or temp_root == "" then temp_root = "." end
local output_path = temp_root .. "/ironflow-image-flip-" .. uuid4() .. ".png"

flow:step("flip", nodes.image_flip({
    path = "${ctx._flow_dir}/../fixtures/ironflow-sample.png",
    direction = "vertical",
    output_path = output_path,
    output_key = "flipped"
}))

flow:step("log", nodes.log({
    message = "Flipped image: ${ctx.flipped}"
})):depends_on("flip")

return flow
