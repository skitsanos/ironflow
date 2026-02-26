-- Demonstrates if_node conditional routing
local flow = Flow.new("conditional_routing")

-- Evaluate condition: is the order amount over 100?
flow:step("check_amount", nodes.if_node({
    condition = "ctx.amount > 100",
    true_route = "high_value",
    false_route = "standard"
}))

-- Only runs when amount > 100
flow:step("high_value_order", nodes.log({
    message = "High-value order: $${ctx.amount} — applying VIP discount",
    level = "info"
})):depends_on("check_amount"):route("high_value")

-- Only runs when amount <= 100
flow:step("standard_order", nodes.log({
    message = "Standard order: $${ctx.amount}",
    level = "info"
})):depends_on("check_amount"):route("standard")

return flow

-- Run with:
--   ironflow run examples/03-control-flow/conditional_routing.lua --context '{"amount": 250}'
--   ironflow run examples/03-control-flow/conditional_routing.lua --context '{"amount": 50}'
