--[[
Simple unified LLM chat request.

Flow:
1) Call `nodes.llm` using provider `openai`.
2) Build a single user message from `prompt`.
3) Print the extracted assistant reply.

Environment variables:
- OPENAI_API_KEY
- OPENAI_BASE_URL (optional, defaults to https://api.openai.com/v1)
]]

local flow = Flow.new("llm_openai_chat")

flow:step("ask", nodes.llm({
    provider = "openai",
    mode = "chat",
    model = "gpt-5-mini",
    prompt = "Hello",
    output_key = "llm_openai"
}))

flow:step("print", nodes.log({
    message = "LLM reply: ${ctx.llm_openai_text}"
})):depends_on("ask")

return flow
