--[[
HTTP + S3 example: generate a presigned PUT URL, upload a local file through it,
and verify the upload through S3 get_object.

Flow:
1. Write a local file with demo text.
2. Read local file content into context.
3. Generate a presigned PUT URL for `raw/temp/demo/presigned_upload.txt`.
4. Upload the file content with an HTTP PUT to the signed URL.
5. Confirm upload by reading the object via `s3_get_object`.
6. Verify local and remote cleanup.
]]

local flow = Flow.new("s3_presigned_upload")

--[[
Step 1: Create a local file we will upload via signed URL.
]]
flow:step("write_source", nodes.write_file({
    path = "/tmp/ironflow_presigned_upload.txt",
    content = "Hello from IronFlow HTTP signed upload example",
    output_key = "source_file"
}))

--[[
Step 2: Read the file content into context for upload.
]]
flow:step("read_source", nodes.read_file({
    path = "/tmp/ironflow_presigned_upload.txt",
    output_key = "payload"
})):depends_on("write_source")

--[[
Step 3: Generate a presigned PUT URL for the destination object.
]]
flow:step("generate_url", nodes.s3_presign_url({
    bucket = env("S3_BUCKET"),
    key = "raw/temp/demo/presigned_upload.txt",
    method = "PUT",
    expires_in = 600,
    content_type = "text/plain",
    output_key = "signed"
})):depends_on("read_source")

--[[
Step 4: Upload content through the presigned URL using HTTP PUT.
]]
flow:step("http_upload", nodes.http_put({
    url = "${ctx.signed_url}",
    headers = { ["Content-Type"] = "text/plain" },
    body = "${ctx.payload_content}",
    body_type = "text",
    output_key = "upload"
})):depends_on("generate_url")

--[[
Step 5: Confirm the object exists and fetch it back from S3.
]]
flow:step("confirm_upload", nodes.s3_get_object({
    bucket = env("S3_BUCKET"),
    key = "raw/temp/demo/presigned_upload.txt",
    encoding = "text",
    output_key = "confirmed"
})):depends_on("http_upload")

--[[
Step 6: Log the verification details.
]]
flow:step("log", nodes.log({
    message = "Uploaded via presigned URL and verified. Size=${ctx.confirmed_size} bytes."
})):depends_on("confirm_upload")

--[[
Step 7: Clean up remote and local artifacts.
]]
flow:step("delete_remote", nodes.s3_delete_object({
    bucket = env("S3_BUCKET"),
    key = "raw/temp/demo/presigned_upload.txt",
    output_key = "remote_delete"
})):depends_on("log")

flow:step("delete_local", nodes.delete_file({
    path = "/tmp/ironflow_presigned_upload.txt",
    output_key = "local_delete"
})):depends_on("delete_remote")

return flow
