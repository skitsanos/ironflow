-- Demonstrates copy_file and move_file nodes

local flow = Flow.new("copy_move_files")

-- Create a source file
flow:step("create", nodes.write_file({
    path = "/tmp/ironflow_copy_test.txt",
    content = "Hello from IronFlow"
}))

-- Copy it
flow:step("copy", nodes.copy_file({
    source = "/tmp/ironflow_copy_test.txt",
    destination = "/tmp/ironflow_copied.txt"
})):depends_on("create")

-- Move the copy to a new name
flow:step("move", nodes.move_file({
    source = "/tmp/ironflow_copied.txt",
    destination = "/tmp/ironflow_moved.txt"
})):depends_on("copy")

-- Verify the moved file exists
flow:step("verify", nodes.read_file({
    path = "/tmp/ironflow_moved.txt",
    output_key = "result"
})):depends_on("move")

flow:step("log", nodes.log({
    message = "File content after copy+move: ${ctx.result_content}"
})):depends_on("verify")

-- Clean up
flow:step("cleanup1", nodes.delete_file({
    path = "/tmp/ironflow_copy_test.txt"
})):depends_on("log")

flow:step("cleanup2", nodes.delete_file({
    path = "/tmp/ironflow_moved.txt"
})):depends_on("log")

return flow
