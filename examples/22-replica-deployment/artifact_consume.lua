-- Restore an artifact descriptor supplied in initial context. The replica
-- acceptance gate invokes this flow directly on replica B, whose cache is
-- isolated from replica A.
-- Remote restore checks cancellation between chunks, including a continuously
-- progressing download. A run deadline (IRONFLOW_MAX_RUN_SECONDS) or an optional
-- step :timeout(seconds) discards staging and leaves existing output unchanged.
local flow = Flow.new("replica_artifact_consume")

flow:step("restore", nodes.write_file({
    path = "${ctx.output_path}",
    source_key = "artifact",
    encoding = "artifact"
}))

return flow
