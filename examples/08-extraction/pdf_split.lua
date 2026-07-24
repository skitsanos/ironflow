-- Effects: retains one UUID-scoped directory of PDF pages under TMPDIR, TMP,
-- TEMP, or `.`.
local flow = Flow.new("pdf_split_example")
local temp_root = env("TMPDIR")
if temp_root == nil or temp_root == "" then temp_root = env("TMP") end
if temp_root == nil or temp_root == "" then temp_root = env("TEMP") end
if temp_root == nil or temp_root == "" then temp_root = "." end
local output_dir = temp_root .. "/ironflow-pdf-split-" .. uuid4()

flow:step("split", nodes.pdf_split({
    path = "${ctx._flow_dir}/../fixtures/ironflow-sample.pdf",
    output_dir = output_dir,
    pages = "1-3"
}))

flow:step("log_result", nodes.log({
    message = "Split into ${ctx.pdf_split_page_count} pages: ${ctx.pdf_split_files}"
})):depends_on("split")

return flow
