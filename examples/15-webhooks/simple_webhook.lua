-- Simple webhook that logs the incoming payload.
--
-- Config (ironflow.yaml):
--   flows_dir: "examples/15-webhooks"
--   webhooks:
--     hello: simple_webhook.lua
--
-- Usage:
--   curl -X POST http://localhost:3000/webhooks/hello \
--     -H "X-API-Key: $IRONFLOW_API_KEY" \
--     -H "Content-Type: application/json" \
--     -d '{"name": "World"}'
--
-- The scalar mapping forwards no HTTP request headers into the workflow.

local flow = Flow.new("simple_webhook")

flow:step("greet", function(ctx)
    local name = ctx.name
    if name == nil then name = "stranger" end
    return { greeting = "Hello, " .. name .. "!" }
end)

flow:step("log_it", nodes.log({
    message = "${ctx.greeting}"
})):depends_on("greet")

return flow
