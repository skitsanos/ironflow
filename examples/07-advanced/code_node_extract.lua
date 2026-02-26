-- Code Node: Extract content from OpenAI Chat Completions response
-- Demonstrates using inline Lua to process API responses
local flow = Flow.new("code_node_extract")

-- Call OpenAI Chat Completions API
flow:step("ask", nodes.http_post({
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

-- Extract just the message content using a code node
flow:step("extract", nodes.code({
    source = [[
        local data = ctx.ai_data
        local reply = data.choices[1].message.content
        local model = data.model
        local tokens = data.usage.total_tokens
        return { reply = reply, model = model, tokens_used = tokens }
    ]]
})):depends_on("ask")

-- Log the extracted reply
flow:step("show", nodes.log({
    message = "AI (${ctx.model}, ${ctx.tokens_used} tokens): ${ctx.reply}",
    level = "info"
})):depends_on("extract")

return flow

-- Run with:
--   ironflow run examples/07-advanced/code_node_extract.lua \
--     --context '{"prompt": "What is the capital of France?"}'
