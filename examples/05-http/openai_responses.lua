-- OpenAI Responses API
-- Uses POST /v1/responses with gpt-5-nano
local flow = Flow.new("openai_responses")

-- Provide a default prompt so the example runs without explicit context.
flow:step("prepare_input", nodes.code({
    source = function(ctx)
        local prompt = ctx.prompt
        if type(prompt) ~= "string" or prompt == "" then
            prompt = "What is the capital of France?"
        end
        return { prompt = prompt }
    end
}))

-- Call the Responses API
flow:step("respond", nodes.http_post({
    url = "https://api.openai.com/v1/responses",
    auth = { type = "bearer", token = env("OPENAI_API_KEY") },
    headers = { ["Content-Type"] = "application/json" },
    body = {
        model = "gpt-5-nano",
        input = {
            { role = "user", content = "${ctx.prompt}" }
        },
        instructions = "You are a helpful assistant. Reply concisely."
    },
    timeout = 30,
    output_key = "response"
})):depends_on("prepare_input")

-- Keep response parsing simple and resilient for responses payload shapes.
flow:step("extract_response", nodes.code({
    source = function(ctx)
        local response = ctx.response or {}
        local output = response.output or {}
        local text = ""

        if type(output) == "table" and #output > 0 then
            local first = output[1]
            if type(first) == "table" then
                local content = first.content
                if type(content) == "string" then
                    text = content
                elseif type(content) == "table" then
                    for _, item in ipairs(content) do
                        if type(item) == "table" and type(item.text) == "string" then
                            text = text .. item.text
                        end
                    end
                end
            end
        end

        if text == "" then
            if type(response.output_text) == "string" then
                text = response.output_text
            elseif type(response.content) == "string" then
                text = response.content
            end
        end

        if text == "" then
            text = json_stringify(response)
        end

        return { response_data = text }
    end
})):depends_on("respond")

-- Log the response
flow:step("show", nodes.log({
    message = "Response: ${ctx.response_data}",
    level = "info"
})):depends_on("extract_response")

return flow

-- Run with:
--   ironflow run examples/05-http/openai_responses.lua \
--     --context '{"prompt": "What is the capital of France?"}'
