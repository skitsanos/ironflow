--[[
S3 example: generate and use a presigned URL for an object in raw/temp.

Flow:
1. Upload a demo object to `raw/temp/demo/presigned.txt`.
2. Generate a presigned GET URL for 10 minutes.
3. Fetch the object content via HTTP GET using the generated URL.
4. Delete the object from S3 to leave the demo directory clean.
]]

local flow = Flow.new("s3_presign_url")

--[[
Step 1: Create the S3 object that we will later access with a presigned URL.
]]
flow:step("upload", nodes.s3_put_object({
    bucket = env("S3_BUCKET"),
    key = "raw/temp/demo/presigned.txt",
    content = "Hello from presigned URL demo",
    output_key = "upload"
}))

--[[
Step 2: Generate a presigned URL for the object.
]]
flow:step("presign", nodes.s3_presign_url({
    bucket = env("S3_BUCKET"),
    key = "raw/temp/demo/presigned.txt",
    method = "GET",
    expires_in = 600,
    output_key = "signed"
})):depends_on("upload")

--[[
Step 3: Use the presigned URL with http_get to verify it can access the object.
]]
flow:step("download_via_url", nodes.http_get({
    url = "${ctx.signed_url}",
    output_key = "download"
})):depends_on("presign")

--[[
Step 4: Remove the test object so the folder is not polluted.
]]
flow:step("delete", nodes.s3_delete_object({
    bucket = env("S3_BUCKET"),
    key = "raw/temp/demo/presigned.txt",
    output_key = "deleted"
})):depends_on("download_via_url")

return flow
