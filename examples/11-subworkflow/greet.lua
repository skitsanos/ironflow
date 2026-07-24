-- greet.lua — A simple reusable subworkflow
-- Expects "name" in context, outputs a greeting message.

local flow = Flow.new("greet")

flow:step("build_greeting", nodes.code({
    source = function(ctx)
        local name = ctx.name
        if name == nil then name = "World" end
        return { greeting = "Hello, " .. name .. "!" }
    end
}))

return flow
