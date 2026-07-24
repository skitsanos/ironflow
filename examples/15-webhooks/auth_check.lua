-- Webhook that validates an explicitly forwarded business signature.
--
-- Config (ironflow.yaml):
--   flows_dir: "examples/15-webhooks"
--   webhooks:
--     auth-check:
--       flow: auth_check.lua
--       forward_headers:
--         - x-webhook-signature
--
-- Usage:
--   export WEBHOOK_SHARED_SECRET="replace-with-a-long-secret"
--   curl -X POST http://localhost:3000/webhooks/auth-check \
--     -H "X-API-Key: $IRONFLOW_API_KEY" \
--     -H "X-Webhook-Signature: $WEBHOOK_SHARED_SECRET" \
--     -H "Content-Type: application/json" \
--     -d '{"action": "deploy"}'

local flow = Flow.new("auth_check")

-- Validate the execution-only signature in place. Never return or log it.
flow:step("validate_auth", function(ctx)
    local signature = ctx._headers and ctx._headers["x-webhook-signature"] or ""
    local expected = env("WEBHOOK_SHARED_SECRET")
    if not expected or expected == "" then
        error("WEBHOOK_SHARED_SECRET is not configured")
    end
    if signature ~= expected then
        error("Invalid webhook signature")
    end
    return { auth_valid = true }
end)

-- Step 2: Process the webhook payload (only runs if auth succeeded)
flow:step("process", function(ctx)
    local action = ctx.action
    if action == nil then action = "unknown" end
    local webhook = ctx._webhook
    if webhook == nil then webhook = "?" end
    return {
        result = "Processed action '" .. action .. "'"
            .. " via webhook '" .. webhook .. "'"
            .. " (authenticated)"
    }
end):depends_on("validate_auth")

-- Step 3: Log the result
flow:step("log_result", nodes.log({
    message = "${ctx.result}"
})):depends_on("process")

return flow
