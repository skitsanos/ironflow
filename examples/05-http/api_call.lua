-- Demonstrates HTTP requests with response processing
local flow = Flow.new("api_call")

-- Fetch a public API
flow:step("fetch", nodes.http_get({
    url = "https://httpbin.org/json",
    output_key = "api"
}))

-- Log the response status
flow:step("check", nodes.log({
    message = "HTTP status: ${ctx.api_status}, success: ${ctx.api_success}",
    level = "info"
})):depends_on("fetch")

return flow

-- Run with:
--   ironflow run examples/05-http/api_call.lua
