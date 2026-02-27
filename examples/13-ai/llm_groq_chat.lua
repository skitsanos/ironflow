--[[
Chat completion with Groq via the unified nodes.llm interface.

Flow:
1) Resolve GROQ_API_KEY from environment.
2) Call nodes.llm with provider="custom" and Groq OpenAI-compatible endpoint.
3) Print the assistant text response.

Environment variables:
- GROQ_API_KEY
]]

local flow = Flow.new("llm_groq_chat")

flow:step("ask", nodes.llm({
    provider = "custom",
    mode = "chat",
    base_url = "https://api.groq.com/openai/v1",
    api_key = env("GROQ_API_KEY"),
    model = "llama-3.1-8b-instant",
    prompt = "Hello",
    output_key = "llm_groq",
}))

flow:step("print", nodes.log({
    message = "GROQ reply: ${ctx.llm_groq_text}",
})):depends_on("ask")

return flow
