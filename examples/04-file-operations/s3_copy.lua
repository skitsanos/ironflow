--[[
S3 example: copy a literal space/percent key and verify contents and listing.

Requirements:
- AWS credentials and region configuration.
- S3_BUCKET naming a disposable bucket where objects may be created, read,
  listed, and deleted.

Effects:
- Uses a UUID-scoped remote prefix. Successful runs delete both objects;
  a failed run may leave objects under the logged prefix.

Flow:
1. Upload source content to a unique example prefix.
2. Copy it to a second key under that prefix.
3. Download the copy and verify its contents.
4. List objects under the same prefix.
5. Delete both source and copy objects.
]]

local flow = Flow.new("s3_copy")
local prefix = "ironflow/examples/s3-copy/" .. uuid4() .. "/"
-- Keys stay literal; the copy node encodes the header, including the '%' byte.
local source_key = prefix .. "original %2F.txt"
local copy_key = prefix .. "copy.txt"

--[[
Step 1: Create initial object directly from text content.
]]
flow:step("upload_source", nodes.s3_put_object({
    bucket = env("S3_BUCKET"),
    key = source_key,
    content = "Original content for copy flow",
    output_key = "source_upload"
}))

--[[
Step 2: Copy source object to a second key.
]]
flow:step("copy", nodes.s3_copy_object({
    source_bucket = env("S3_BUCKET"),
    source_key = source_key,
    bucket = env("S3_BUCKET"),
    key = copy_key,
    output_key = "copy"
})):depends_on("upload_source")

--[[
Step 3: Download and verify the exact content at the destination key.
]]
flow:step("download_copy", nodes.s3_get_object({
    bucket = env("S3_BUCKET"),
    key = copy_key,
    encoding = "text",
    output_key = "downloaded"
})):depends_on("copy")

flow:step("verify_copy", function(ctx)
    assert(ctx.downloaded_content == "Original content for copy flow", "Copied content mismatch")
    return { copy_verified = true }
end):depends_on("download_copy")

--[[
Step 4: List results.
]]
flow:step("list", nodes.s3_list_objects({
    bucket = env("S3_BUCKET"),
    prefix = prefix,
    output_key = "demo_objects"
})):depends_on("verify_copy")

--[[
Step 5: Delete both objects so the demo leaves no artifacts.
]]
flow:step("delete_source", nodes.s3_delete_object({
    bucket = env("S3_BUCKET"),
    key = source_key,
    output_key = "source_deleted"
})):depends_on("list")

flow:step("delete_copy", nodes.s3_delete_object({
    bucket = env("S3_BUCKET"),
    key = copy_key,
    output_key = "copy_deleted"
})):depends_on("delete_source")

--[[
Step 6: Log completed state.
]]
flow:step("log", nodes.log({
    message = "Copy demo complete. Prefix " .. prefix .. " contained ${ctx.demo_objects_count} object(s) before cleanup"
})):depends_on("delete_copy")

return flow
