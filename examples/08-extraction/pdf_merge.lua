-- Effects: publishes one immutable source artifact and retains one UUID-scoped
-- merged PDF under TMPDIR, TMP, TEMP, or `.`.
local flow = Flow.new("pdf_merge_example")
local temp_root = env("TMPDIR")
if temp_root == nil or temp_root == "" then temp_root = env("TMP") end
if temp_root == nil or temp_root == "" then temp_root = env("TEMP") end
if temp_root == nil or temp_root == "" then temp_root = "." end
local output_path = temp_root .. "/ironflow-pdf-merge-" .. uuid4() .. ".pdf"

flow:step("cache_source", nodes.read_file({
    path = "${ctx._flow_dir}/../fixtures/ironflow-sample.pdf",
    output_key = "source_pdf",
    encoding = "artifact",
    mime_type = "application/pdf"
}))

flow:step("build_sources", function(ctx)
    return {
        merge_sources = { ctx.source_pdf_artifact, ctx.source_pdf_artifact }
    }
end):depends_on("cache_source")

flow:step("merge", nodes.pdf_merge({
    source_key = "merge_sources",
    output_path = output_path
})):depends_on("build_sources")

flow:step("log_result", nodes.log({
    message = "Merged PDF saved to ${ctx.pdf_merge_path} with ${ctx.pdf_merge_page_count} pages"
})):depends_on("merge")

return flow
