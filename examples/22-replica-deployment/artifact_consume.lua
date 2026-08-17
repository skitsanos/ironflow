-- Restore an artifact descriptor supplied in initial context. The replica
-- acceptance gate invokes this flow directly on replica B, whose cache is
-- isolated from replica A.
local flow = Flow.new("replica_artifact_consume")

flow:step("restore", nodes.write_file({
    path = "${ctx.output_path}",
    source_key = "artifact",
    encoding = "artifact"
}))

return flow
