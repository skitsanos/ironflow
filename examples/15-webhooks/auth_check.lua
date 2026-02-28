-- Webhook that validates an Authorization header before processing.
--
-- Config (ironflow.yaml):
--   flows_dir: "examples/15-webhooks"
--   webhooks:
--     auth-check: auth_check.lua
--
-- Usage:
--   curl -X POST http://localhost:3000/webhooks/auth-check \
--     -H "Authorization: Bearer my-secret-token" \
--     -H "Content-Type: application/json" \
--     -d '{"action": "deploy"}'

local flow = Flow.new("auth_check")

-- Step 1: Extract and validate the Authorization header
flow:step("validate_auth", function(ctx)
    local auth = ""
    if ctx._headers then
        auth = ctx._headers.authorization or ctx._headers.Authorization or ""
    elseif ctx.headers then
        auth = ctx.headers.authorization or ctx.headers.Authorization or ""
    else
        auth = ctx.authorization or ctx.Authorization or ""
    end

    if auth == "" then
        error("Missing Authorization header")
    end

    -- Extract the token (strip "Bearer " prefix)
    local token = auth:match("^Bearer%s+(.+)$")
    if not token then
        error("Invalid Authorization format — expected 'Bearer <token>'")
    end

    -- In a real app you'd verify the token against a database or JWT library.
    -- Return values get merged into context for downstream steps.
    return { auth_token = token, auth_valid = true }
end)

-- Step 2: Process the webhook payload (only runs if auth succeeded)
flow:step("process", function(ctx)
    return {
        result = "Processed action '" .. (ctx.action or "unknown") .. "'"
            .. " via webhook '" .. (ctx._webhook or "?") .. "'"
            .. " (authenticated)"
    }
end):depends_on("validate_auth")

-- Step 3: Log the result
flow:step("log_result", nodes.log({
    message = "${ctx.result}"
})):depends_on("process")

return flow
