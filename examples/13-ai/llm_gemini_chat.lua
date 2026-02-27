--[[
Simple Gemini example using the unified `nodes.llm`.

Flow:
1. Call `nodes.llm` with `provider = "custom"` to target Gemini's
   OpenAI-compatible endpoint.
2. Send a short `Hello` prompt using the `gemini-3-flash-preview` model.
3. Print the assistant reply.

Environment variables:
- GEMINI_API_KEY
]]

local flow = Flow.new("llm_gemini_chat")

flow:step("ask", nodes.llm({
    provider = "custom",
    mode = "chat",
    model = "gemini-3-flash-preview",
    prompt = "Hello",
    base_url = "https://generativelanguage.googleapis.com/v1beta/openai",
    auth_type = "bearer",
    api_key = env("GEMINI_API_KEY"),
    output_key = "llm_gemini"
}))

flow:step("print", nodes.log({
    message = "Gemini reply: ${ctx.llm_gemini_text}"
})):depends_on("ask")

return flow
