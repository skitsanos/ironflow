local flow = Flow.new("literal_item_child")

flow:step("echo", function(ctx)
    return { received = ctx.job, position = ctx.ordinal }
end)

return flow
