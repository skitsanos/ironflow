--[[
S3 example: copy an object and verify results via listing.

Flow:
1. Upload source content to `raw/temp/demo/original.txt`.
2. Copy it to `raw/temp/demo/copy.txt`.
3. List objects in `raw/temp/demo/`.
4. Delete both source and copy objects.
]]

local flow = Flow.new("s3_copy")

--[[
Step 1: Create initial object directly from text content.
]]
flow:step("upload_source", nodes.s3_put_object({
    bucket = env("S3_BUCKET"),
    key = "raw/temp/demo/original.txt",
    content = "Original content for copy flow",
    output_key = "source_upload"
}))

--[[
Step 2: Copy source object to a second key.
]]
flow:step("copy", nodes.s3_copy_object({
    source_bucket = env("S3_BUCKET"),
    source_key = "raw/temp/demo/original.txt",
    bucket = env("S3_BUCKET"),
    key = "raw/temp/demo/copy.txt",
    output_key = "copy"
})):depends_on("upload_source")

--[[
Step 3: List results.
]]
flow:step("list", nodes.s3_list_objects({
    bucket = env("S3_BUCKET"),
    prefix = "raw/temp/demo/",
    output_key = "demo_objects"
})):depends_on("copy")

--[[
Step 4: Delete both objects so the demo leaves no artifacts.
]]
flow:step("delete_source", nodes.s3_delete_object({
    bucket = env("S3_BUCKET"),
    key = "raw/temp/demo/original.txt",
    output_key = "source_deleted"
})):depends_on("list")

flow:step("delete_copy", nodes.s3_delete_object({
    bucket = env("S3_BUCKET"),
    key = "raw/temp/demo/copy.txt",
    output_key = "copy_deleted"
})):depends_on("delete_source")

--[[
Step 5: Log completed state.
]]
flow:step("log", nodes.log({
    message = "Copy demo complete. Objects in raw/temp/demo/: ${ctx.demo_objects_count}"
})):depends_on("delete_copy")

return flow
