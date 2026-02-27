--[[
This example demonstrates OAuth client-credentials flow followed by a chat completion call.

Flow:
1. Fetch access token from OAuth token endpoint (form-encoded).
2. Build the chat completion endpoint from `OAUTH_BASE_URL`.
3. Call OpenAI-compatible chat completion with `gpt-5-mini` (customize to `gpt-5` if desired).
4. Extract and log the assistant reply.

Environment variables used:
- OAUTH_TOKEN_URL
- OAUTH_CLIENT_ID
- OAUTH_CLIENT_SECRET
- OAUTH_SCOPE (optional)
- OAUTH_BASE_URL (example: https://provider.example.com for endpoint at /chat/completions)
]]

local flow = Flow.new("oauth_chat_completion")

--[[
Step 1:
Exchange client credentials for a bearer token using form encoding.
]]
flow:step("token", nodes.http_post({
    url = env("OAUTH_TOKEN_URL"),
    body_type = "form",
    body = {
        grant_type = "client_credentials",
        client_id = env("OAUTH_CLIENT_ID"),
        client_secret = env("OAUTH_CLIENT_SECRET"),
        scope = env("OAUTH_SCOPE")
    },
    output_key = "token_request"
}))

--[[
Step 2:
Parse the OAuth response and keep the token.
]]
flow:step("extract_token", nodes.code({
    source = function()
        local token_payload = ctx.token_request_data
        if type(token_payload) ~= "table" or type(token_payload.access_token) ~= "string" then
            return { error = "access_token not found" }
        end

        return {
            access_token = token_payload.access_token,
            token_type = token_payload.token_type or "Bearer"
        }
    end
})):depends_on("token")

--[[
Step 3:
Build `${base}/chat/completions` without forcing `/v1`.
]]
flow:step("chat_url", nodes.code({
    source = function()
        local base = env("OAUTH_BASE_URL")
        if type(base) ~= "string" or base == "" then
            return { error = "OAUTH_BASE_URL is required" }
        end

        local trimmed = base:gsub("^%s+", ""):gsub("%s+$", "")
        local endpoint = trimmed
        if not trimmed:match("/chat/completions$") then
            endpoint = trimmed .. (trimmed:match("/$") and "" or "/") .. "chat/completions"
        end

        return { chat_url = endpoint }
    end
})):depends_on("extract_token")

--[[
Step 4:
Call chat completions with model `gpt-5-mini`.
]]
flow:step("chat", nodes.http_post({
    url = "${ctx.chat_url}",
    auth = {
        type = "bearer",
        token = "${ctx.access_token}"
    },
    body = {
        model = "gpt-5-mini",
        messages = {
            { role = "system", content = "You are a concise assistant." },
            { role = "user", content = "Hello" }
        },
        max_tokens = 64,
        temperature = 0.2
    },
    timeout = 30,
    output_key = "chat"
})):depends_on("chat_url")

--[[
Step 5:
Log raw response and then extract the first assistant reply from
supported response shapes:
- choices[1].message.content (standard OpenAI shape)
- choices[1].text (alternate model/provider shape)
- output_text (text-centric provider shape)
]]
flow:step("show_chat_raw", nodes.log({
    message = "OAuth chat raw response: ${ctx.chat_data}",
    level = "info"
})):depends_on("chat")

flow:step("extract_reply", nodes.code({
    source = function()
        local function stringify_parts(parts)
            if type(parts) ~= "table" then
                return nil
            end

            local acc = {}
            for _, part in ipairs(parts) do
                if type(part) == "string" then
                    table.insert(acc, part)
                elseif type(part) == "table" then
                    if type(part.text) == "string" then
                        table.insert(acc, part.text)
                    elseif type(part.content) == "string" then
                        table.insert(acc, part.content)
                    end
                end
            end

            if #acc == 0 then
                return nil
            end
            return table.concat(acc)
        end

        local data = ctx.chat_data
        local reply
        if type(data) == "table" and type(data.choices) == "table" and data.choices[1] then
            local first = data.choices[1]
            if type(first.message) == "table" then
                if type(first.message.content) == "string" then
                    reply = first.message.content
                elseif type(first.message.content) == "table" then
                    reply = stringify_parts(first.message.content)
                end
            end

            if reply == nil then
                if type(first.text) == "string" then
                    reply = first.text
                elseif type(first.content) == "table" then
                    reply = stringify_parts(first.content)
                end
            end
        end
        if reply == nil and type(data.output_text) == "string" then
            reply = data.output_text
        end

        local normalized_reply = reply
        if type(reply) ~= "string" then
            normalized_reply = data and tostring(data) or nil
        end

        return {
            chat_reply = normalized_reply or "<no reply>",
            chat_model = data.model or "unknown",
            chat_tokens = data.usage and data.usage.total_tokens
        }
    end
})):depends_on("show_chat_raw")

--[[
Step 6:
Log assistant reply.
]]
flow:step("show", nodes.log({
    message = "OAuth chat reply: ${ctx.chat_reply}"
})):depends_on("extract_reply")

return flow
