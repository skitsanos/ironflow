-- Demonstrates HTTP requests with authentication
local flow = Flow.new("authenticated_request")

-- Bearer token auth (token from environment variable)
flow:step("bearer_call", nodes.http_get({
    url = "https://httpbin.org/bearer",
    auth = { type = "bearer", token = env("API_TOKEN") or "demo-token" },
    output_key = "bearer"
}))

-- Basic auth
flow:step("basic_call", nodes.http_get({
    url = "https://httpbin.org/basic-auth/user/pass",
    auth = { type = "basic", username = "user", password = "pass" },
    output_key = "basic"
}))

-- Both run in parallel, then log results
flow:step("summary", nodes.log({
    message = "Bearer: ${ctx.bearer_status}, Basic: ${ctx.basic_status}",
    level = "info"
})):depends_on("bearer_call", "basic_call")

return flow

-- Run with:
--   ironflow run examples/05-http/authenticated_request.lua
