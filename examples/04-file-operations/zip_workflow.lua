-- Demonstrates file archival workflow: create -> list -> extract -> cleanup
local flow = Flow.new("zip_workflow")

flow:step("prepare_dir", nodes.shell_command({
    command = "mkdir",
    args = {"-p", "/tmp/ironflow_zip_demo"}
}))

flow:step("prepare", nodes.write_file({
    path = "/tmp/ironflow_zip_demo/alpha.txt",
    content = "alpha"
})):depends_on("prepare_dir")

flow:step("prepare_nested", nodes.write_file({
    path = "/tmp/ironflow_zip_demo/beta.txt",
    content = "beta"
})):depends_on("prepare")

flow:step("create_zip", nodes.zip_create({
    source = "/tmp/ironflow_zip_demo",
    zip_path = "/tmp/ironflow_zip_demo.zip",
    include_root = false,
    compression = "deflated"
})):depends_on("prepare_nested")

flow:step("list_zip", nodes.zip_list({
    path = "/tmp/ironflow_zip_demo.zip",
    output_key = "zip_members"
})):depends_on("create_zip")

flow:step("extract_zip", nodes.zip_extract({
    path = "/tmp/ironflow_zip_demo.zip",
    destination = "/tmp/ironflow_zip_demo_out",
    output_key = "extracted_items",
    overwrite = true
})):depends_on("list_zip")

flow:step("report", nodes.log({
    message = "Zip has ${ctx.zip_members_count} entries, extracted to ${ctx.zip_extract_destination}",
    level = "info"
})):depends_on("extract_zip")

flow:step("cleanup", nodes.delete_file({
    path = "/tmp/ironflow_zip_demo.zip"
})):depends_on("report")

return flow

-- Run with:
--   ironflow run examples/04-file-operations/zip_workflow.lua
