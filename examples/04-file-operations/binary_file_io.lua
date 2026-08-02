--
-- Binary file I/O
--
-- Demonstrates an explicit Base64 provider boundary and the preferred
-- disk-backed artifact handoff for binary workflow data.
--
-- Effects:
-- - Creates and removes two UUID-scoped PNGs under TMPDIR, TMP, TEMP, or `.`.
-- - Publishes one immutable artifact under IRONFLOW_ARTIFACT_DIR; it is not
--   automatically pruned.
-- - A failed run may leave that uniquely named file for inspection.
--

local flow = Flow.new("binary_file_io")
local temp_root = env("TMPDIR")
if temp_root == nil or temp_root == "" then temp_root = env("TMP") end
if temp_root == nil or temp_root == "" then temp_root = env("TEMP") end
if temp_root == nil or temp_root == "" then temp_root = "." end
local image_path = temp_root .. "/ironflow-binary-file-" .. uuid4() .. ".png"
local restored_path = temp_root .. "/ironflow-restored-file-" .. uuid4() .. ".png"

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

-- The normal workflow handoff keeps bytes on disk and returns only a small,
-- immutable content-addressed descriptor.
flow:step("cache_binary", nodes.read_file({
    path = image_path,
    output_key = "cached",
    encoding = "artifact",
    mime_type = "image/png"
})):depends_on("write_png")

-- Restore without materializing the artifact bytes in workflow context.
flow:step("restore_binary", nodes.write_file({
    path = restored_path,
    source_key = "cached_artifact"
})):depends_on("cache_binary")

flow:step("read_restored", nodes.read_file({
    path = restored_path,
    output_key = "restored",
    encoding = "base64"
})):depends_on("restore_binary")

-- Verify both the explicit Base64 and artifact-backed round trips.
flow:step("verify", function(ctx)
    return {
        roundtrip_ok = (ctx.img_data == ctx.result_content),
        artifact_roundtrip_ok = (ctx.img_data == ctx.restored_content),
        artifact_ok = ctx.cached_artifact ~= nil
            and ctx.cached_artifact.artifact_uri ~= nil
            and ctx.cached_artifact.size_bytes > 0
    }
end):depends_on("read_back", "read_restored")

flow:step("done", nodes.log({
    message = "Base64 OK: ${ctx.roundtrip_ok}; artifact restore OK: ${ctx.artifact_roundtrip_ok}; artifact descriptor OK: ${ctx.artifact_ok}"
})):depends_on("verify")

flow:step("cleanup", nodes.delete_file({
    path = image_path
})):depends_on("done")

flow:step("cleanup_restored", nodes.delete_file({
    path = restored_path
})):depends_on("cleanup")

return flow
