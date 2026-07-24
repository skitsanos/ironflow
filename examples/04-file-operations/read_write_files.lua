-- Demonstrates file read/write operations
-- Effects: creates and removes one UUID-scoped file under TMPDIR, TMP, TEMP, or `.`.
-- A failed run may leave that uniquely named file for inspection.
local flow = Flow.new("file_operations")
local temp_root = env("TMPDIR")
if temp_root == nil or temp_root == "" then temp_root = env("TMP") end
if temp_root == nil or temp_root == "" then temp_root = env("TEMP") end
if temp_root == nil or temp_root == "" then temp_root = "." end
local file_path = temp_root .. "/ironflow-read-write-" .. uuid4() .. ".txt"

-- Write a file
flow:step("write", nodes.write_file({
    path = file_path,
    content = "Hello from IronFlow!\nTimestamp: ${ctx.timestamp}"
}))

-- Read it back
flow:step("read", nodes.read_file({
    path = file_path,
    output_key = "result"
})):depends_on("write")

-- Log the content
flow:step("show", nodes.log({
    message = "File content: ${ctx.result_content}",
    level = "info"
})):depends_on("read")

-- List the selected temporary directory (non-recursive)
flow:step("list", nodes.list_directory({
    path = temp_root,
    output_key = "tmp_files"
})):depends_on("write")

-- Clean up
flow:step("cleanup", nodes.delete_file({
    path = file_path
})):depends_on("read", "list")

return flow

-- Run with:
--   ironflow run examples/04-file-operations/read_write_files.lua \
--     --context '{"timestamp": "2026-02-26"}'
