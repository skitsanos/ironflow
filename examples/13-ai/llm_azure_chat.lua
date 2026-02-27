--[[
Unified LLM call for Azure deployments.

Flow:
1) Call `nodes.llm` with `provider = "azure"` and `mode = "chat"`.
2) Use environment values for endpoint/version/deployment/key.
3) Log the normalized assistant reply.

Environment variables:
- AZURE_OPENAI_ENDPOINT
- AZURE_OPENAI_API_KEY
- AZURE_OPENAI_API_VERSION
- AZURE_OPENAI_CHAT_DEPLOYMENT
]]

local flow = Flow.new("llm_azure_chat")

flow:step("ask", nodes.llm({
    provider = "azure",
    mode = "chat",
    prompt = "Hello",
    output_key = "llm_azure"
}))

flow:step("print", nodes.log({
    message = "Azure LLM reply: ${ctx.llm_azure_text}"
})):depends_on("ask")

return flow
