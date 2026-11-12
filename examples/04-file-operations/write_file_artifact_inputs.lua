--
-- write_file artifact inputs
--
-- `artifact` names bytes in the artifact store: a descriptor or a canonical
-- artifact://sha256/... URI. A local path is rejected, so the node cannot be
-- used to copy an arbitrary file. Copy a file by path with copy_file, or store
-- it first with read_file and encoding = "artifact".
--
-- Effects:
-- - Creates and removes three UUID-scoped text files under TMPDIR, TMP, TEMP,
--   or `.`.
-- - Publishes one immutable artifact under IRONFLOW_ARTIFACT_DIR; it is not
--   automatically pruned.
-- - The `path_as_artifact` step fails by design and is resolved by its
--   recovery step, so the run finishes successfully.
--

local flow = Flow.new("write_file_artifact_inputs")
local temp_root = env("TMPDIR")
if temp_root == nil or temp_root == "" then temp_root = env("TMP") end
if temp_root == nil or temp_root == "" then temp_root = env("TEMP") end
if temp_root == nil or temp_root == "" then temp_root = "." end
local run_id = uuid4()
local source_path = temp_root .. "/ironflow-artifact-source-" .. run_id .. ".txt"
local restored_path = temp_root .. "/ironflow-artifact-restored-" .. run_id .. ".txt"
local copied_path = temp_root .. "/ironflow-artifact-copied-" .. run_id .. ".txt"
local rejected_path = temp_root .. "/ironflow-artifact-rejected-" .. run_id .. ".txt"

flow:step("write_source", nodes.write_file({
    path = source_path,
    content = "report body"
}))

-- Store the file once; only the small descriptor enters workflow context.
flow:step("store_source", nodes.read_file({
    path = source_path,
    encoding = "artifact",
    mime_type = "text/plain",
    output_key = "stored"
})):depends_on("write_source")

-- Accepted: a canonical artifact URI. The descriptor object
-- (`source_key = "stored_artifact"`) works the same way.
flow:step("restore_by_uri", nodes.write_file({
    path = restored_path,
    artifact = "${ctx.stored_artifact.artifact_uri}"
})):depends_on("store_source")

-- Rejected: a local path is not an artifact. Nothing is written.
flow:step("path_as_artifact", nodes.write_file({
    path = rejected_path,
    artifact = source_path
})):depends_on("write_source"):on_error("explain_rejection")

flow:step("explain_rejection", function(ctx)
    return { rejection = ctx._error_message }
end)

-- The supported way to copy a file that is named by path.
flow:step("copy_by_path", nodes.copy_file({
    source = source_path,
    destination = copied_path
})):depends_on("path_as_artifact")

flow:step("read_restored", nodes.read_file({
    path = restored_path,
    output_key = "restored"
})):depends_on("restore_by_uri")

flow:step("read_copied", nodes.read_file({
    path = copied_path,
    output_key = "copied"
})):depends_on("copy_by_path")

flow:step("verify", function(ctx)
    assert(ctx.restored_content == "report body", "artifact restore changed the bytes")
    assert(ctx.copied_content == "report body", "copy_file changed the bytes")
    assert(
        string.find(ctx.rejection or "", "artifact input must be an artifact descriptor", 1, true),
        "a path passed as an artifact was not rejected"
    )
    return { artifact_inputs_ok = true }
end):depends_on("read_restored", "read_copied")

flow:step("done", nodes.log({
    message = "Artifact restore and copy_file OK: ${ctx.artifact_inputs_ok}; path rejected with: ${ctx.rejection}"
})):depends_on("verify")

flow:step("cleanup_source", nodes.delete_file({
    path = source_path
})):depends_on("done")

flow:step("cleanup_restored", nodes.delete_file({
    path = restored_path
})):depends_on("cleanup_source")

flow:step("cleanup_copied", nodes.delete_file({
    path = copied_path
})):depends_on("cleanup_restored")

return flow
