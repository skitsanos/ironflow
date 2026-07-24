--[[
S3 example: upload from a local file, read back, list under a UUID-scoped
remote prefix, then delete the uploaded object.

Requirements:
- AWS credentials and region configuration.
- S3_BUCKET naming a disposable bucket where objects may be created/deleted.
- Permission to list account buckets (`s3:ListAllMyBuckets` on AWS S3).

Effects:
- Uses UUID-scoped local and remote paths. Successful runs remove both; a
  failed run may leave attributable state under those unique paths.

Flow:
1. Verify bucket-list access before creating any state.
2. Create a local text file.
3. Upload it with `s3_put_object`.
4. Download it and list the objects under this run's prefix.
5. Delete the uploaded object.
6. Log the result and remove the temporary local file.
]]

local flow = Flow.new("s3_put_get_list")
local token = uuid4()
local temp_root = env("TMPDIR")
if temp_root == nil or temp_root == "" then temp_root = env("TMP") end
if temp_root == nil or temp_root == "" then temp_root = env("TEMP") end
if temp_root == nil or temp_root == "" then temp_root = "." end
local source_path = temp_root .. "/ironflow-s3-" .. token .. ".txt"
local prefix = "ironflow/examples/s3-put-get/" .. token .. "/"
local object_key = prefix .. "payload.txt"

--[[
Step 1: Verify account-level bucket listing before any local or remote mutation.
]]
flow:step("list_buckets", nodes.s3_list_buckets({
    output_key = "account_buckets"
}))

--[[
Step 2: Prepare a file that will be used as upload source.
]]
flow:step("prepare", nodes.write_file({
    path = source_path,
    content = "IronFlow S3 demo payload\n"
})):depends_on("list_buckets")

--[[
Step 3: Upload the local file to this run's S3 prefix.
]]
flow:step("upload", nodes.s3_put_object({
    bucket = env("S3_BUCKET"),
    key = object_key,
    source_path = source_path,
    content_type = "text/plain",
    output_key = "uploaded"
})):depends_on("prepare")

--[[
Step 4: Download the uploaded object for verification.
]]
flow:step("download", nodes.s3_get_object({
    bucket = env("S3_BUCKET"),
    key = object_key,
    encoding = "text",
    output_key = "downloaded"
})):depends_on("upload")

--[[
Step 4: List objects under the same prefix used by upload.
]]
flow:step("list", nodes.s3_list_objects({
    bucket = env("S3_BUCKET"),
    prefix = prefix,
    output_key = "objects"
})):depends_on("upload")

--[[
Step 5: Delete the uploaded object.
]]
flow:step("remove", nodes.s3_delete_object({
    bucket = env("S3_BUCKET"),
    key = object_key,
    output_key = "deleted"
})):depends_on("download", "list")

--[[
Step 6: Inspect the outcome in logs.
]]
flow:step("log", nodes.log({
    message = "S3 demo complete: account buckets=${ctx.account_buckets_count}, bucket=${ctx.uploaded_bucket}, key=${ctx.uploaded_key}, size=${ctx.downloaded_size}, prefix objects=${ctx.objects_count}"
})):depends_on("remove")

--[[
Clean up local temp file.
]]
flow:step("cleanup", nodes.delete_file({
    path = source_path
})):depends_on("log")

return flow
