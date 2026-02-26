-- OpenAI Chat Completions with function handler to extract the reply
local flow = Flow.new("openai_with_extract")

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
    output_key = "ai"
}))

-- Extract just the reply using a function handler
flow:step("extract", function(ctx)
    local data = ctx.ai_data
    return {
        reply = data.choices[1].message.content,
        model = data.model,
        tokens = data.usage.total_tokens
    }
end):depends_on("chat")

-- Log the clean reply
flow:step("show", nodes.log({
    message = "${ctx.reply}",
    level = "info"
})):depends_on("extract")

return flow

-- Run with:
--   ironflow run examples/05-http/openai_with_extract.lua \
--     --context '{"prompt": "What is the capital of France?"}'
