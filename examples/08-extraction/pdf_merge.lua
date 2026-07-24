-- Effects: retains one UUID-scoped PDF under TMPDIR, TMP, TEMP, or `.`.
local flow = Flow.new("pdf_merge_example")
local temp_root = env("TMPDIR")
if temp_root == nil or temp_root == "" then temp_root = env("TMP") end
if temp_root == nil or temp_root == "" then temp_root = env("TEMP") end
if temp_root == nil or temp_root == "" then temp_root = "." end
local output_path = temp_root .. "/ironflow-pdf-merge-" .. uuid4() .. ".pdf"

flow:step("merge", nodes.pdf_merge({
    files = {
        "${ctx._flow_dir}/../fixtures/ironflow-sample.pdf",
        "${ctx._flow_dir}/../fixtures/ironflow-sample.pdf"
    },
    output_path = output_path
}))

flow:step("log_result", nodes.log({
    message = "Merged PDF saved to ${ctx.pdf_merge_path} with ${ctx.pdf_merge_page_count} pages"
})):depends_on("merge")

return flow
