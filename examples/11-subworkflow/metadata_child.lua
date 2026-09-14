local flow = Flow.new("metadata_child")

flow:step("produce", function(ctx)
    local should_fail = ctx.should_fail or (ctx.item and ctx.item.should_fail) or false
    return {
        should_fail = should_fail,
        success = should_fail,
        flow = "domain-flow",
        error = "domain-note",
        value = 42
    }
end)

flow:step("finish", function(ctx)
    assert(not ctx.should_fail, "intentional child failure")
    return { finished = true }
end):depends_on("produce")

return flow
