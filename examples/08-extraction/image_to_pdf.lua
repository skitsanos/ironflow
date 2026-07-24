-- Effects: retains one UUID-scoped PDF under TMPDIR, TMP, TEMP, or `.`.
local flow = Flow.new("image_to_pdf_demo")
local temp_root = env("TMPDIR")
if temp_root == nil or temp_root == "" then temp_root = env("TMP") end
if temp_root == nil or temp_root == "" then temp_root = env("TEMP") end
if temp_root == nil or temp_root == "" then temp_root = "." end
local output_path = temp_root .. "/ironflow-image-to-pdf-" .. uuid4() .. ".pdf"

-- Convert existing images into a single PDF.
flow:step("convert", nodes.image_to_pdf({
    sources = {
        { path = "${ctx._flow_dir}/../fixtures/ironflow-sample.png" },
        { path = "${ctx._flow_dir}/../fixtures/ironflow-sample.png" },
    },
    output_path = output_path,
    output_key = "report_pdf"
}))

flow:step("log", nodes.log({
    message = "Created ${ctx.report_pdf} with ${ctx.report_pdf_count} page(s)"
})):depends_on("convert")

return flow
