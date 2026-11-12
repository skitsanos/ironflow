-- Webhook that reads one allowlisted request header and routes on the body.
--
-- Config (ironflow.yaml):
--   flows_dir: "examples/15-webhooks"
--   webhooks:
--     tenant-order:
--       flow: forwarded_header.lua
--       forward_headers:
--         - x-tenant-token
--
-- Usage:
--   curl -X POST http://localhost:3000/webhooks/tenant-order \
--     -H "X-API-Key: $IRONFLOW_API_KEY" \
--     -H "X-Tenant-Token: tenant-7f3a9c21;region=eu" \
--     -H "Content-Type: application/json" \
--     -d '{"amount": 250}'
--
-- A forwarded header is confidential for the whole run. Its complete value
-- and each component of at least eight bytes ("tenant-7f3a9c21") are replaced
-- by "[REDACTED]" in step output and never reach run records. Short
-- components ("eu"), the header name, numbers and routing decisions are left
-- alone, so the guarded step below runs for any header value.

local flow = Flow.new("forwarded_header")

-- ctx._headers exists only while the run executes. Use the value in place and
-- return facts derived from it, not the value itself.
flow:step("inspect", function(ctx)
    local token = ctx._headers and ctx._headers["x-tenant-token"]
    return {
        tenant_token_present = token ~= nil,
        -- Stored and passed on as "[REDACTED]": do not rely on a raw copy.
        tenant_token_copy = token,
        region = "eu"
    }
end)

-- The webhook body must carry a numeric "amount": an ordering guard fails on
-- a missing value instead of skipping the step.
flow:step_if("ctx.amount > 100", "needs_approval", nodes.log({
    message = "Order of ${ctx.amount} needs approval (region ${ctx.region})"
})):depends_on("inspect")

return flow
