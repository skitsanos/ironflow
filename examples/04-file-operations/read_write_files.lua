-- Demonstrates file read/write operations
local flow = Flow.new("file_operations")

-- Write a file
flow:step("write", nodes.write_file({
    path = "/tmp/ironflow_test.txt",
    content = "Hello from IronFlow!\nTimestamp: ${ctx.timestamp}"
}))

-- Read it back
flow:step("read", nodes.read_file({
    path = "/tmp/ironflow_test.txt",
    output_key = "result"
})):depends_on("write")

-- Log the content
flow:step("show", nodes.log({
    message = "File content: ${ctx.result_content}",
    level = "info"
})):depends_on("read")

-- List the /tmp directory (non-recursive)
flow:step("list", nodes.list_directory({
    path = "/tmp",
    output_key = "tmp_files"
})):depends_on("write")

-- Clean up
flow:step("cleanup", nodes.delete_file({
    path = "/tmp/ironflow_test.txt"
})):depends_on("read", "list")

return flow

-- Run with:
--   ironflow run examples/04-file-operations/read_write_files.lua \
--     --context '{"timestamp": "2026-02-26"}'
