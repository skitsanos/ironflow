-- OpenAI Responses API
-- Uses POST /v1/responses with gpt-4o-mini
local flow = Flow.new("openai_responses")

-- Call the Responses API
flow:step("respond", nodes.http_post({
    url = "https://api.openai.com/v1/responses",
    auth = { type = "bearer", token = env("OPENAI_API_KEY") },
    headers = { ["Content-Type"] = "application/json" },
    body = {
        model = "gpt-4o-mini",
        input = "${ctx.prompt}",
        instructions = "You are a helpful assistant. Reply concisely."
    },
    timeout = 30,
    output_key = "response"
}))

-- Log the response
flow:step("show", nodes.log({
    message = "Response: ${ctx.response_data}",
    level = "info"
})):depends_on("respond")

return flow

-- Run with:
--   ironflow run examples/05-http/openai_responses.lua \
--     --context '{"prompt": "What is the capital of France?"}'
