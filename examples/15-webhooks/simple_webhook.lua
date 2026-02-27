-- Simple webhook that logs the incoming payload.
--
-- Config (ironflow.yaml):
--   flows_dir: "examples/15-webhooks"
--   webhooks:
--     hello: simple_webhook.lua
--
-- Usage:
--   curl -X POST http://localhost:3000/webhooks/hello \
--     -H "Content-Type: application/json" \
--     -d '{"name": "World"}'

local flow = Flow.new("simple_webhook")

flow:step("greet", function(ctx)
    local name = ctx.name or "stranger"
    return { greeting = "Hello, " .. name .. "!" }
end)

flow:step("log_it", nodes.log({
    message = "${ctx.greeting}"
})):depends_on("greet")

return flow
