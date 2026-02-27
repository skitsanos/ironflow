--[[
This example performs sentiment analysis on a VTT conversation file using OAuth-backed
chat completions.

Flow:
1. Extract transcript from `data/samples/interview.vtt`.
2. Exchange OAuth client credentials for an access token.
3. Build `${OAUTH_BASE_URL}/chat/completions`.
4. Send the full transcript to `gpt-5-mini` with a sentiment analysis prompt.
5. Normalize the first assistant reply from supported response shapes.
6. Log sentiment result.

Environment variables required:
- OAUTH_TOKEN_URL
- OAUTH_CLIENT_ID
- OAUTH_CLIENT_SECRET
- OAUTH_SCOPE (optional)
- OAUTH_BASE_URL (example: https://provider.example.com for endpoint at /chat/completions)
]]

local flow = Flow.new("vtt_sentiment_analysis")

--[[ Step 1: parse the VTT sample and keep transcript + cue list. ]]
flow:step("extract_vtt", nodes.extract_vtt({
    path = "data/samples/interview.vtt",
    format = "text",
    output_key = "interview_transcript",
    metadata_key = "interview_meta"
}))

--[[ Step 2: request OAuth access token using form-encoded body. ]]
flow:step("get_access_token", nodes.http_post({
    url = env("OAUTH_TOKEN_URL"),
    body_type = "form",
    body = {
        grant_type = "client_credentials",
        client_id = env("OAUTH_CLIENT_ID"),
        client_secret = env("OAUTH_CLIENT_SECRET"),
        scope = env("OAUTH_SCOPE")
    },
    output_key = "token_request"
})):depends_on("extract_vtt")

--[[ Step 3: extract token fields into a compact context object. ]]
flow:step("token", nodes.code({
    source = function()
        local payload = ctx.token_request_data
        if type(payload) ~= "table" or type(payload.access_token) ~= "string" then
            return { error = "access_token not found" }
        end

        return {
            access_token = payload.access_token,
            token_type = payload.token_type or "Bearer"
        }
    end
})):depends_on("get_access_token")

--[[ Step 4: build the chat completion endpoint without hardcoded `/v1`. ]]
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
})):depends_on("token")

--[[ Step 5: run sentiment analysis with model `gpt-5-mini`. ]]
flow:step("analyze_sentiment", nodes.http_post({
    url = "${ctx.chat_url}",
    auth = {
        type = "bearer",
        token = "${ctx.access_token}"
    },
    body = {
    model = "gpt-5-mini",
        messages = {
            {
                role = "system",
                content = [[You are a concise sentiment analyst.
Return JSON only with: overall_sentiment, confidence, per_speaker.
`per_speaker` is an array of {speaker, sentiment, rationale}.]]
            },
            {
                role = "user",
                content = "Please analyze the following interview transcript and return sentiment results:\n\n"
                    .. "${ctx.interview_transcript}"
            }
        },
        reasoning_effort = "low",
        temperature = 0.3
    },
    timeout = 45,
    output_key = "chat"
})):depends_on("chat_url")

--[[ Step 6: normalize assistant reply to one simple `sentiment_analysis` string. ]]
flow:step("normalize_reply", nodes.code({
    source = function()
        local function stringify_parts(parts)
            if type(parts) ~= "table" then
                return nil
            end

            local output = {}
            for _, part in ipairs(parts) do
                if type(part) == "string" then
                    table.insert(output, part)
                elseif type(part) == "table" then
                    if type(part.text) == "string" then
                        table.insert(output, part.text)
                    elseif type(part.content) == "string" then
                        table.insert(output, part.content)
                    end
                end
            end

            if #output == 0 then
                return nil
            end
            return table.concat(output)
        end

        local response = ctx.chat_data
        local reply
        if type(response) == "table" and type(response.choices) == "table" and response.choices[1] then
            local first = response.choices[1]
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

        if reply == nil and type(response.output_text) == "string" then
            reply = response.output_text
        end

        local safe_reply = reply
        if type(reply) ~= "string" then
            safe_reply = response and tostring(response) or nil
        end

        return {
            sentiment_analysis = safe_reply or "<no sentiment response>",
            sentiment_model = response and response.model or "unknown"
        }
    end
})):depends_on("analyze_sentiment")

--[[ Step 7: log summary and a small subset of parsed cue metadata. ]]
flow:step("log_analysis", nodes.log({
    message = "Sentiment model=${ctx.sentiment_model} | response=${ctx.sentiment_analysis}"
})):depends_on("normalize_reply")

return flow
