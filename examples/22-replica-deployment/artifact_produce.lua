-- Publish a fixture through the shared S3-compatible artifact backend. The
-- replica acceptance gate invokes this flow directly on replica A.
local flow = Flow.new("replica_artifact_produce")

flow:step("publish", nodes.read_file({
    path = "${ctx._flow_dir}/artifact-fixture.txt",
    encoding = "artifact",
    mime_type = "text/plain",
    output_key = "produced"
}))

return flow
