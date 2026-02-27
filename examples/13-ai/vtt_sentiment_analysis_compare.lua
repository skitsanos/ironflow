--[[
This example compares `gpt-5-mini` and `gpt-5` on the same VTT transcript.

Flow:
1. Extract transcript from `data/samples/interview.vtt`.
2. Exchange OAuth client credentials for an access token.
3. Build `${OAUTH_BASE_URL}/chat/completions`.
4. Call chat completions with `gpt-5-mini`.
5. Call chat completions with `gpt-5`.
6. Extract assistant replies and compare with a deterministic rubric:
   - overall sentiment weight
   - per-speaker sentiment weight
   - confidence value when present
7. Log results so you can judge answer quality side-by-side.

Environment variables required:
- OAUTH_TOKEN_URL
- OAUTH_CLIENT_ID
- OAUTH_CLIENT_SECRET
- OAUTH_SCOPE (optional)
- OAUTH_BASE_URL (example: https://provider.example.com for endpoint at /chat/completions)
]]

local flow = Flow.new("vtt_sentiment_analysis_compare")

--[[ Step 1: parse the VTT sample and keep transcript + cue metadata. ]]
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

--[[ Shared system prompt for both runs. ]]
local sentiment_prompt = [[You are a concise sentiment analyst.
Return JSON only with: overall_sentiment, confidence, per_speaker.
`per_speaker` is an array of {speaker, sentiment, rationale}.]]

--[[ Step 5: run sentiment analysis with `gpt-5-mini`. ]]
flow:step("analyze_mini", nodes.http_post({
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
                content = sentiment_prompt
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
    output_key = "mini_chat"
})):depends_on("chat_url")

--[[ Step 6: run sentiment analysis with `gpt-5`. ]]
flow:step("analyze_full", nodes.http_post({
    url = "${ctx.chat_url}",
    auth = {
        type = "bearer",
        token = "${ctx.access_token}"
    },
    body = {
        model = "gpt-5",
        messages = {
            {
                role = "system",
                content = sentiment_prompt
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
    output_key = "full_chat"
})):depends_on("chat_url")

--[[ Step 7: normalize both replies and apply a deterministic quality rubric. ]]
flow:step("compare", nodes.code({
    source = function()
        local sentiment_scores = {
            ["very positive"] = 3,
            ["positive"] = 2,
            ["neutral"] = 1,
            ["negative"] = -2,
            ["very negative"] = -3
        }

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

        local function first_reply(response)
            if type(response) ~= "table" then
                return nil
            end

            local raw
            if type(response.choices) == "table" and response.choices[1] then
                local first = response.choices[1]
                if type(first.message) == "table" then
                    if type(first.message.content) == "string" then
                        raw = first.message.content
                    elseif type(first.message.content) == "table" then
                        raw = stringify_parts(first.message.content)
                    end
                end
                if raw == nil then
                    if type(first.text) == "string" then
                        raw = first.text
                    elseif type(first.content) == "table" then
                        raw = stringify_parts(first.content)
                    end
                end
            end

            if raw == nil and type(response.output_text) == "string" then
                raw = response.output_text
            end

            return raw
        end

        local function safe_parse_json(value)
            if type(value) ~= "string" then
                return nil
            end
            local ok, parsed = pcall(json_parse, value)
            if not ok or type(parsed) ~= "table" then
                return nil
            end
            return parsed
        end

        local function sentiment_weight(value)
            if type(value) ~= "string" then
                return 0
            end
            return sentiment_scores[string.lower(value)] or 0
        end

        local function average_table(values)
            if type(values) ~= "table" or #values == 0 then
                return 0, 0
            end

            local sum = 0
            local count = 0
            for i = 1, #values do
                if type(values[i]) == "number" then
                    sum = sum + values[i]
                    count = count + 1
                end
            end

            if count == 0 then
                return 0, 0
            end

            return sum / count, count
        end

        local function rubric(payload)
            if type(payload) ~= "table" then
                return {
                    overall = 0,
                    per_speaker = 0,
                    confidence = 0,
                    confidence_used = false,
                    per_speaker_count = 0,
                    total_score = 0
                }
            end

            local overall = sentiment_weight(payload.overall_sentiment)

            local confidence = 0
            local confidence_used = false
            local confidence_value = payload.confidence
            if type(confidence_value) == "number" then
                confidence = math.max(0, math.min(1, confidence_value))
                confidence_used = true
            end

            local per_speaker_scores = {}
            local speakers = payload.per_speaker
            if type(speakers) == "table" then
                for _, speaker_entry in ipairs(speakers) do
                    if type(speaker_entry) == "table" then
                        table.insert(per_speaker_scores, sentiment_weight(speaker_entry.sentiment))
                    end
                end
            end

            local per_speaker_score, per_speaker_count = average_table(per_speaker_scores)
            local total_score = (overall * 1.5) + (per_speaker_score * 1.0) + confidence

            return {
                overall = overall,
                per_speaker = per_speaker_score,
                confidence = confidence,
                confidence_used = confidence_used,
                per_speaker_count = per_speaker_count,
                total_score = total_score
            }
        end

        local function pick(model_a_name, model_a_payload, model_b_name, model_b_payload)
            local a = rubric(model_a_payload)
            local b = rubric(model_b_payload)

            local winner = "undetermined"
            if a.total_score > b.total_score then
                winner = model_a_name
            elseif b.total_score > a.total_score then
                winner = model_b_name
            end

            return winner, a, b
        end

        local mini_reply = first_reply(ctx.mini_chat_data) or "<no reply>"
        local full_reply = first_reply(ctx.full_chat_data) or "<no reply>"

        local mini_payload = safe_parse_json(mini_reply)
        local full_payload = safe_parse_json(full_reply)

        local better_model, mini_scores, full_scores = pick("gpt-5-mini", mini_payload, "gpt-5", full_payload)

        return {
            mini_sentiment = mini_reply,
            full_sentiment = full_reply,
            mini_confidence = mini_scores.confidence,
            full_confidence = full_scores.confidence,
            mini_score = mini_scores.total_score,
            full_score = full_scores.total_score,
            mini_overall_score = mini_scores.overall,
            full_overall_score = full_scores.overall,
            mini_per_speaker_score = mini_scores.per_speaker,
            full_per_speaker_score = full_scores.per_speaker,
            mini_per_speaker_count = mini_scores.per_speaker_count,
            full_per_speaker_count = full_scores.per_speaker_count,
            better_by_rubric = better_model,
            candidate_models = { "gpt-5-mini", "gpt-5" }
        }
    end
})):depends_on("analyze_mini"):depends_on("analyze_full")

--[[ Step 8: log results for manual comparison. ]]
flow:step("log_compare", nodes.log({
    message = "gpt-5-mini confidence=${ctx.mini_confidence} score=${ctx.mini_score}, gpt-5 confidence=${ctx.full_confidence} score=${ctx.full_score}, suggested=${ctx.better_by_rubric}"
})):depends_on("compare")

flow:step("log_results", nodes.log({
    message = "mini=${ctx.mini_sentiment}\nfull=${ctx.full_sentiment}"
})):depends_on("compare")

return flow
