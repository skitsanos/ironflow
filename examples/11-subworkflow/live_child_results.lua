local flow = Flow.new("live_child_results")

flow:step("call_child", nodes.subworkflow({
    flow = "large_result_child.lua",
    input = {},
    output_key = "child",
    on_error = "fail_fast"
}))

-- The parent receives all bytes even when persisted inspection uses a marker.
flow:step("verify", function(ctx)
    assert(type(ctx.child.payload) == "string", "child payload was truncated")
    assert(#ctx.child.payload == 3 * 1024 * 1024, "child payload changed")
    return { child_bytes = #ctx.child.payload, live_result_verified = true }
end):depends_on("call_child")

return flow
