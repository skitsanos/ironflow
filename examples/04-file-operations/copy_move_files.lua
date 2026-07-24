-- Demonstrates copy_file and move_file nodes
-- Effects: creates and removes UUID-scoped files under TMPDIR, TMP, TEMP, or `.`.
-- A failed run may leave only its uniquely named files for inspection.

local flow = Flow.new("copy_move_files")
local temp_root = env("TMPDIR")
if temp_root == nil or temp_root == "" then temp_root = env("TMP") end
if temp_root == nil or temp_root == "" then temp_root = env("TEMP") end
if temp_root == nil or temp_root == "" then temp_root = "." end
local path_prefix = temp_root .. "/ironflow-copy-move-" .. uuid4()
local source_path = path_prefix .. "-source.txt"
local copied_path = path_prefix .. "-copied.txt"
local moved_path = path_prefix .. "-moved.txt"

-- Create a source file
flow:step("create", nodes.write_file({
    path = source_path,
    content = "Hello from IronFlow"
}))

-- Copy it
flow:step("copy", nodes.copy_file({
    source = source_path,
    destination = copied_path
})):depends_on("create")

-- Move the copy to a new name
flow:step("move", nodes.move_file({
    source = copied_path,
    destination = moved_path
})):depends_on("copy")

-- Verify the moved file exists
flow:step("verify", nodes.read_file({
    path = moved_path,
    output_key = "result"
})):depends_on("move")

flow:step("log", nodes.log({
    message = "File content after copy+move: ${ctx.result_content}"
})):depends_on("verify")

-- Clean up
flow:step("cleanup1", nodes.delete_file({
    path = source_path
})):depends_on("log")

flow:step("cleanup2", nodes.delete_file({
    path = moved_path
})):depends_on("log")

return flow
