-- Webhook protected by fail-closed HMAC-SHA256 ingress verification.
--
-- Config (ironflow.yaml):
--   flows_dir: "examples/15-webhooks"
--   webhooks:
--     auth-check:
--       flow: auth_check.lua
--       signature:
--         type: hmac_sha256
--         header: x-hub-signature-256
--         secret_env: WEBHOOK_SIGNING_SECRET
--         prefix: sha256=
--
-- Usage:
--   export WEBHOOK_SIGNING_SECRET="replace-with-a-long-random-secret"
--   body='{"action":"deploy"}'
--   signature=$(BODY="$body" bun -e '
--     const h = new Bun.CryptoHasher("sha256", process.env.WEBHOOK_SIGNING_SECRET);
--     console.log("sha256=" + h.update(process.env.BODY).digest("hex"));
--   ')
--   curl --request POST http://localhost:3000/webhooks/auth-check \
--     -H "X-API-Key: $IRONFLOW_API_KEY" \
--     -H "X-Hub-Signature-256: $signature" \
--     -H "Content-Type: application/json" \
--     --data-binary "$body"

local flow = Flow.new("auth_check")

-- The flow starts only after IronFlow verifies the exact request bytes. The
-- signing secret and signature header never enter workflow context.
flow:step("process", function(ctx)
    local action = ctx.action
    if action == nil then action = "unknown" end
    local webhook = ctx._webhook
    if webhook == nil then webhook = "?" end
    return {
        result = "Processed action '" .. action .. "'"
            .. " via webhook '" .. webhook .. "'"
            .. " (HMAC authenticated)"
    }
end)

-- Step 2: Log the result
flow:step("log_result", nodes.log({
    message = "${ctx.result}"
})):depends_on("process")

return flow
