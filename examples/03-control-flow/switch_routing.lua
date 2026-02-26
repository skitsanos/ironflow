-- Demonstrates switch_node multi-case routing
local flow = Flow.new("switch_routing")

-- Route based on the value of ctx.tier
flow:step("check_tier", nodes.switch_node({
    value = "ctx.tier",
    cases = {
        free = "free_path",
        pro = "pro_path",
        enterprise = "enterprise_path"
    },
    default = "free_path"
}))

flow:step("handle_free", nodes.log({
    message = "Free tier: limited to 10 requests/day",
    level = "info"
})):depends_on("check_tier"):route("free_path")

flow:step("handle_pro", nodes.log({
    message = "Pro tier: 1000 requests/day with priority support",
    level = "info"
})):depends_on("check_tier"):route("pro_path")

flow:step("handle_enterprise", nodes.log({
    message = "Enterprise tier: unlimited requests with SLA",
    level = "info"
})):depends_on("check_tier"):route("enterprise_path")

return flow

-- Run with:
--   ironflow run examples/03-control-flow/switch_routing.lua --context '{"tier": "pro"}'
--   ironflow run examples/03-control-flow/switch_routing.lua --context '{"tier": "enterprise"}'
--   ironflow run examples/03-control-flow/switch_routing.lua --context '{"tier": "unknown"}'
