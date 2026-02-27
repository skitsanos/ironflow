--[[
S3 example: upload from local file, read back, list under raw/temp/demo/,
then delete the uploaded object.

Flow:
1. Create a local text file.
2. Upload it with `s3_put_object` using `source_path`.
3. Download it with `s3_get_object`.
4. List all objects under `raw/temp/demo/`.
5. Delete the uploaded object.
6. Remove the temporary local file.
]]

local flow = Flow.new("s3_put_get_list")

--[[
Step 1: Prepare a file that will be used as upload source.
]]
flow:step("prepare", nodes.write_file({
    path = "/tmp/ironflow_s3_demo.txt",
    content = "IronFlow S3 demo payload\n"
}))

--[[
Step 2: Upload local file to S3 folder `raw/temp/demo/`.
]]
flow:step("upload", nodes.s3_put_object({
    bucket = env("S3_BUCKET"),
    key = "raw/temp/demo/manual.txt",
    source_path = "/tmp/ironflow_s3_demo.txt",
    content_type = "text/plain",
    output_key = "uploaded"
})):depends_on("prepare")

--[[
Step 3: Download the uploaded object for verification.
]]
flow:step("download", nodes.s3_get_object({
    bucket = env("S3_BUCKET"),
    key = "raw/temp/demo/manual.txt",
    encoding = "text",
    output_key = "downloaded"
})):depends_on("upload")

--[[
Step 4: List objects under the same prefix used by upload.
]]
flow:step("list", nodes.s3_list_objects({
    bucket = env("S3_BUCKET"),
    prefix = "raw/temp/demo/",
    output_key = "objects"
})):depends_on("upload")

--[[
Step 5: Delete the uploaded object.
]]
flow:step("remove", nodes.s3_delete_object({
    bucket = env("S3_BUCKET"),
    key = "raw/temp/demo/manual.txt",
    output_key = "deleted"
})):depends_on("download", "list")

--[[
Step 6: Inspect the outcome in logs.
]]
flow:step("log", nodes.log({
    message = "S3 demo complete: bucket=${ctx.uploaded_bucket}, key=${ctx.uploaded_key}, size=${ctx.downloaded_size}, objects=${ctx.objects_count}"
})):depends_on("remove")

--[[
Clean up local temp file.
]]
flow:step("cleanup", nodes.delete_file({
    path = "/tmp/ironflow_s3_demo.txt"
})):depends_on("log")

return flow
