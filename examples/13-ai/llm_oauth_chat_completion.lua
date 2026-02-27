--[[
OAuth client-credentials + nodes.llm chat completion example.

Flow:
1) Get access token from OAuth token endpoint (form-encoded).
2) Parse token from response.
3) Call protected chat endpoint using `nodes.llm` with provider `custom`.
4) Print the assistant reply.

Environment variables:
- OAUTH_TOKEN_URL
- OAUTH_CLIENT_ID
- OAUTH_CLIENT_SECRET
- OAUTH_SCOPE (optional)
- OAUTH_BASE_URL
]]

local flow = Flow.new("llm_oauth_chat_completion")

-- Exchange client credentials for access token.
flow:step("token", nodes.http_post({
    url = env("OAUTH_TOKEN_URL"),
    body_type = "form",
    body = {
        grant_type = "client_credentials",
        client_id = env("OAUTH_CLIENT_ID"),
        client_secret = env("OAUTH_CLIENT_SECRET"),
        scope = env("OAUTH_SCOPE")
    },
    output_key = "oauth_token_request"
}))

-- Keep token in context for llm call.
flow:step("extract_token", nodes.code({
    source = function()
        local token_payload = ctx.oauth_token_request_data
        if type(token_payload) ~= "table" or type(token_payload.access_token) ~= "string" then
            return { error = "access_token not found" }
        end

        return {
            access_token = token_payload.access_token,
            token_type = token_payload.token_type or "Bearer",
        }
    end
})):depends_on("token")

-- Chat using the same flow endpoint path as earlier example: `${OAUTH_BASE_URL}/chat/completions`.
flow:step("chat", nodes.llm({
    provider = "custom",
    mode = "chat",
    base_url = env("OAUTH_BASE_URL"),
    chat_path = "/chat/completions",
    api_key = "${ctx.access_token}",
    auth_type = "bearer",
    model = "gpt-5-mini",
    prompt = "Hello",
    output_key = "oauth_llm",
    temperature = 0.3,
    max_tokens = 64
})):depends_on("extract_token")

flow:step("show", nodes.log({
    message = "OAuth LLM chat reply: ${ctx.oauth_llm_text}"
})):depends_on("chat")

return flow
