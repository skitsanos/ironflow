-- IF-136: a Lua handler can exclude an unused 200,000-number array.
-- Requirements: none. Run with:
--   ironflow run examples/07-advanced/context_projection.lua
local flow = Flow.new("context_projection")

flow:step("prepare", nodes.code({
    context_keys = {},
    source = function()
        return {
            payload_json = "[" .. string.rep("1,", 199999) .. "1]",
            count = 200000
        }
    end
}))

-- Parse in a native node so the large array never crosses the Lua output budget.
flow:step("parse", nodes.json_parse({
    source_key = "payload_json",
    output_key = "unused"
})):depends_on("prepare")

flow:step("inspect", function(ctx)
    assert(ctx.unused == nil)
    return { inspected_count = ctx.count }
end):context_keys({"count"}):depends_on("parse")

flow:step("constant", function(ctx)
    assert(next(ctx) == nil)
    return { ready = true }
end):context_keys({}):depends_on("inspect")

flow:step("done", nodes.log({
    message = "Inspected count: ${ctx.inspected_count}; ready: ${ctx.ready}"
})):depends_on("constant")

return flow
