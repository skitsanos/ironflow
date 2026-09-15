-- Effects: retains a UUID-scoped directory of PDF slices under TMPDIR, TMP,
-- TEMP, or `.`. Use a new output directory for each run; grouped output refuses
-- existing files. Completed slices survive a later failure.
-- Override input_path and pages_spec in the initial context for your own PDF.
local flow = Flow.new("pdf_split_grouped_example")
local temp_root = env("TMPDIR")
if temp_root == nil or temp_root == "" then temp_root = env("TMP") end
if temp_root == nil or temp_root == "" then temp_root = env("TEMP") end
if temp_root == nil or temp_root == "" then temp_root = "." end
local output_dir = temp_root .. "/ironflow-pdf-groups-" .. uuid4()

flow:step("select", function(ctx)
    local input_path = ctx.input_path
    if input_path == nil then
        input_path = ctx._flow_dir .. "/../fixtures/ironflow-sample.pdf"
    end
    local pages_spec = ctx.pages_spec
    if pages_spec == nil then pages_spec = "all" end
    return { input_path = input_path, pages_spec = pages_spec }
end)

flow:step("split", nodes.pdf_split({
    path = "${ctx.input_path}",
    output_dir = output_dir,
    pages = "${ctx.pages_spec}",
    pages_per_file = 30
})):depends_on("select")

flow:step("log_result", nodes.log({
    message = "Grouped ${ctx.pdf_split_page_count} pages: ${ctx.pdf_split_parts}"
})):depends_on("split")

return flow
