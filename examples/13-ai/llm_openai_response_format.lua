--[[
OpenAI response format demo with `nodes.llm`.

Flow:
1) Send a chat prompt with `response_format = { type = "json_object" }`.
2) Send a chat prompt with `response_format = { type = "json_schema", ... }`.
3) Print both extracted JSON replies.

Environment variables:
- OPENAI_API_KEY
- OPENAI_BASE_URL (optional, defaults to https://api.openai.com/v1)
]]

local flow = Flow.new("llm_openai_response_format")

flow:step("json_object", nodes.llm({
    provider = "openai",
    mode = "chat",
    model = "gpt-5-mini",
    prompt = "Return a JSON object with keys `language` and `topic` for this text: 'Rust is fast and safe.'",
    output_key = "openai_json_object",
    extra = {
        response_format = {
            type = "json_object"
        }
    }
}))

flow:step("json_schema", nodes.llm({
    provider = "openai",
    mode = "chat",
    model = "gpt-5-mini",
    prompt = "Return a JSON object for a simple task with fields `sentiment` and `confidence`.",
    output_key = "openai_json_schema",
    extra = {
        response_format = {
            type = "json_schema",
            json_schema = {
                name = "sentiment_schema",
                strict = true,
                schema = {
                    type = "object",
                    properties = {
                        sentiment = { type = "string", enum = { "positive", "neutral", "negative" } },
                        confidence = { type = "number", minimum = 0, maximum = 1 }
                    },
                    required = { "sentiment", "confidence" },
                    additionalProperties = false
                }
            }
        }
    }
})):depends_on("json_object")

flow:step("parse", nodes.code({
    source = function()
        local object_reply = json_parse(ctx.openai_json_object_text)
        local schema_reply = json_parse(ctx.openai_json_schema_text)

        return {
            object_language = object_reply.language,
            object_topic = object_reply.topic,
            schema_sentiment = schema_reply.sentiment,
            schema_confidence = schema_reply.confidence,
        }
    end
})):depends_on("json_schema")

flow:step("print", nodes.log({
    message = "JSON object response: ${ctx.openai_json_object_text}\nJSON schema response: ${ctx.openai_json_schema_text}\nParsed => language=${ctx.object_language}, topic=${ctx.object_topic}, sentiment=${ctx.schema_sentiment}, confidence=${ctx.schema_confidence}",
})):depends_on("parse")

return flow
