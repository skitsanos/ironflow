--
-- Binary file I/O
--
-- Demonstrates reading and writing binary files using base64 encoding.
-- A small PNG image is created in context, written as binary, then read back.
--
-- Effects:
-- - Creates and removes one UUID-scoped PNG under TMPDIR, TMP, TEMP, or `.`.
-- - A failed run may leave that uniquely named file for inspection.
--

local flow = Flow.new("binary_file_io")
local temp_root = env("TMPDIR")
if temp_root == nil or temp_root == "" then temp_root = env("TMP") end
if temp_root == nil or temp_root == "" then temp_root = env("TEMP") end
if temp_root == nil or temp_root == "" then temp_root = "." end
local image_path = temp_root .. "/ironflow-binary-file-" .. uuid4() .. ".png"

-- Create a 1x1 transparent PNG as base64 in context
flow:step("create_data", function(ctx)
    return {
        img_data = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPj/HwADBwIAMCbHYQAAAABJRU5ErkJggg=="
    }
end)

-- Write binary file from context
flow:step("write_png", nodes.write_file({
    path = image_path,
    source_key = "img_data",
    encoding = "base64"
})):depends_on("create_data")

-- Read the binary file back as base64
flow:step("read_back", nodes.read_file({
    path = image_path,
    output_key = "result",
    encoding = "base64"
})):depends_on("write_png")

-- Verify round-trip
flow:step("verify", function(ctx)
    return {
        roundtrip_ok = (ctx.img_data == ctx.result_content)
    }
end):depends_on("read_back")

flow:step("done", nodes.log({
    message = "Binary round-trip OK: ${ctx.roundtrip_ok}"
})):depends_on("verify")

flow:step("cleanup", nodes.delete_file({
    path = image_path
})):depends_on("done")

return flow
