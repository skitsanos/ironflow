--
-- Binary file I/O
--
-- Demonstrates reading and writing binary files using base64 encoding.
-- A small PNG image is created in context, written as binary, then read back.
--

local flow = Flow.new("binary_file_io")

-- Create a 1x1 transparent PNG as base64 in context
flow:step("create_data", function(ctx)
    return {
        img_data = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPj/HwADBwIAMCbHYQAAAABJRU5ErkJggg=="
    }
end)

-- Write binary file from context
flow:step("write_png", nodes.write_file({
    path = "/tmp/ironflow_test.png",
    source_key = "img_data",
    encoding = "base64"
})):depends_on("create_data")

-- Read the binary file back as base64
flow:step("read_back", nodes.read_file({
    path = "/tmp/ironflow_test.png",
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

return flow
