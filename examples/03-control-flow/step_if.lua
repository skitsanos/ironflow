-- Demonstrates step_if: conditional step shorthand
-- Instead of manually wiring if_node + depends_on + route,
-- step_if does it in one call.
local flow = Flow.new("step_if_demo")

-- Only runs when score > 50
flow:step_if("ctx.score > 50", "bonus", nodes.log({
    message = "Bonus unlocked! Score: ${ctx.score}"
}))

-- step_if with a function handler
flow:step_if("ctx.vip", "vip_greeting", function(ctx)
    return { greeting = "Welcome back, VIP " .. ctx.name .. "!" }
end)

-- Regular step that always runs
flow:step("done", nodes.log({
    message = "Processing complete"
}))

return flow

-- Run with:
--   ironflow run examples/03-control-flow/step_if.lua --context '{"score": 80, "vip": true, "name": "Alice"}'
--   ironflow run examples/03-control-flow/step_if.lua --context '{"score": 30, "vip": false, "name": "Bob"}'
