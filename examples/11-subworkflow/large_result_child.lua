local flow = Flow.new("large_result_child")

flow:step("produce", function()
    return { payload = string.rep("x", 3 * 1024 * 1024) }
end)

return flow
