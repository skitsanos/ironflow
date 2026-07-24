-- fire_and_forget.lua — Launch a subworkflow without waiting

local flow = Flow.new("fire_and_forget")

flow:step("prepare", nodes.code({
    source = function(ctx)
        local name = ctx.name
        if name == nil then
            name = "Background"
        end
        return { name = name }
    end
}))

flow:step("async_greet", nodes.subworkflow({
    flow = "greet.lua",
    wait = false,           -- don't wait for completion
    input = {
        name = "name"
    }
})):depends_on("prepare")

flow:step("continue_work", nodes.log({
    message = "Fired subworkflow in background, continuing immediately",
    level = "info"
})):depends_on("async_greet")

return flow

-- Run with:
--   ironflow run examples/11-subworkflow/fire_and_forget.lua
