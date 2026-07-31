-- Extract text and metadata from an HTML file
-- Effects: creates and removes one UUID-scoped HTML file under TMPDIR, TMP,
-- TEMP, or `.`; a failed run may leave it for inspection.

local flow = Flow.new("extract_html")
local temp_root = env("TMPDIR")
if temp_root == nil or temp_root == "" then temp_root = env("TMP") end
if temp_root == nil or temp_root == "" then temp_root = env("TEMP") end
if temp_root == nil or temp_root == "" then temp_root = "." end
local html_path = temp_root .. "/ironflow-extract-html-" .. uuid4() .. ".html"

-- First create an HTML file to extract from
flow:step("create_html", nodes.write_file({
    path = html_path,
    content = "<html><head><title>Test Page</title></head><body><h1>Hello</h1><p>This is a test paragraph.</p></body></html>"
}))

-- Extract text from the HTML
flow:step("extract", nodes.extract_html({
    path = html_path,
    format = "text",
    output_key = "html_text",
    metadata_key = "html_meta"
})):depends_on("create_html")

flow:step("log_result", nodes.log({
    message = "Extracted '${ctx.html_meta.title}': ${ctx.html_text}"
})):depends_on("extract")

-- Clean up
flow:step("cleanup", nodes.delete_file({
    path = html_path
})):depends_on("log_result")

return flow
