-- Demonstrates file archival workflow: create -> list -> extract -> cleanup
-- Platform: requires `mkdir` on PATH.
-- Effects: retains UUID-scoped source and extracted directories under TMPDIR,
-- TMP, TEMP, or `.` for inspection; removes the intermediate ZIP on success.
local flow = Flow.new("zip_workflow")
local temp_root = env("TMPDIR")
if temp_root == nil or temp_root == "" then temp_root = env("TMP") end
if temp_root == nil or temp_root == "" then temp_root = env("TEMP") end
if temp_root == nil or temp_root == "" then temp_root = "." end
local run_path = temp_root .. "/ironflow-zip-" .. uuid4()
local source_dir = run_path .. "-source"
local zip_path = run_path .. ".zip"
local output_dir = run_path .. "-extracted"

flow:step("prepare_dir", nodes.shell_command({
    cmd = "mkdir",
    args = {"-p", source_dir}
}))

flow:step("prepare", nodes.write_file({
    path = source_dir .. "/alpha.txt",
    content = "alpha"
})):depends_on("prepare_dir")

flow:step("prepare_second", nodes.write_file({
    path = source_dir .. "/beta.txt",
    content = "beta"
})):depends_on("prepare")

flow:step("create_zip", nodes.zip_create({
    source = source_dir,
    zip_path = zip_path,
    include_root = false,
    compression = "deflated",
    max_entries = 16,
    max_depth = 4,
    max_total_uncompressed_bytes = 1024
})):depends_on("prepare_second")

flow:step("list_zip", nodes.zip_list({
    path = zip_path,
    output_key = "zip_members",
    max_entries = 16,
    max_total_uncompressed_bytes = 1024
})):depends_on("create_zip")

flow:step("extract_zip", nodes.zip_extract({
    path = zip_path,
    destination = output_dir,
    output_key = "extracted_items",
    overwrite = false,
    max_entries = 16,
    max_depth = 4,
    max_total_uncompressed_bytes = 1024
})):depends_on("list_zip")

flow:step("report", nodes.log({
    message = "Zip has ${ctx.zip_members_count} entries, extracted to ${ctx.zip_extract_destination}",
    level = "info"
})):depends_on("extract_zip")

flow:step("cleanup", nodes.delete_file({
    path = zip_path
})):depends_on("report")

return flow

-- Run with:
--   ironflow run examples/04-file-operations/zip_workflow.lua
