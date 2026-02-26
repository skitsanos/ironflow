-- OpenAI Chat Completions API
-- Uses POST /v1/chat/completions with gpt-4o-mini
local flow = Flow.new("openai_chat_completions")

-- Call the Chat Completions API
flow:step("chat", nodes.http_post({
    url = "https://api.openai.com/v1/chat/completions",
    auth = { type = "bearer", token = env("OPENAI_API_KEY") },
    headers = { ["Content-Type"] = "application/json" },
    body = {
        model = "gpt-4o-mini",
        messages = {
            { role = "system", content = "You are a helpful assistant. Reply concisely." },
            { role = "user", content = "${ctx.prompt}" }
        },
        temperature = 0.7,
        max_tokens = 256
    },
    timeout = 30,
    output_key = "completions"
}))

-- Extract the reply text
flow:step("show", nodes.log({
    message = "Response: ${ctx.completions_data}",
    level = "info"
})):depends_on("chat")

return flow

-- Run with:
--   ironflow run examples/05-http/openai_chat_completions.lua \
--     --context '{"prompt": "Explain recursion in one sentence."}'
