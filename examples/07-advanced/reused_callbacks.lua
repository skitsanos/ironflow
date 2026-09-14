local flow = Flow.new("reused_callbacks")

local function advance(value)
    return { count = (value.count or 0) + 1 }
end

flow:step("prepare", function()
    return { items = { { count = 10 }, { count = 20 } } }
end)

flow:step("first", advance):depends_on("prepare")
flow:step_if("ctx.count == 1", "second", advance):depends_on("first")
flow:step("third", nodes.code({ source = advance })):depends_on("second")
flow:step("each", nodes.foreach({
    source_key = "items",
    transform = advance,
    output_key = "advanced_items"
})):depends_on("third")

flow:step("verify", function(ctx)
    assert(ctx.count == 3)
    assert(ctx.advanced_items[1].count == 11)
    assert(ctx.advanced_items[2].count == 21)
    return { reused_callbacks_verified = true }
end):depends_on("each")

return flow
