--[[
Function calling with nodes.llm + Lua tool definitions.

Flow:
1) Call `nodes.llm` in chat mode with a function tool definition.
2) Parse tool call details from `*_tool_calls` and execute a local fallback weather lookup.
3) Ask the model for one final user-facing sentence using the tool result.

Environment variables:
- OPENAI_API_KEY
- OPENAI_BASE_URL (optional, defaults to https://api.openai.com/v1)
]]

local flow = Flow.new("llm_openai_function_tools")

flow:step("ask", nodes.llm({
    provider = "openai",
    mode = "chat",
    model = "gpt-5-mini",
    output_key = "weather_tool",
    messages = {
        {
            role = "user",
            content = "What is the weather in Paris today?",
        },
    },
    tools = {
        {
            type = "function",
            ["function"] = {
                name = "get_weather",
                description = "Get the current weather for a city.",
                parameters = {
                    type = "object",
                    properties = {
                        city = {
                            type = "string",
                            description = "City name to query.",
                        },
                    },
                    required = { "city" },
                    additionalProperties = false,
                },
            },
        },
    },
    tool_choice = "required",
}))

flow:step("run_tool", nodes.code({
    source = function()
        local calls = ctx.weather_tool_tool_calls
        if type(calls) ~= "table" or #calls == 0 then
            return {
                weather_tool_executed = false,
                weather_tool_error = "No tool call returned by model.",
            }
        end

        local call = calls[1]
        local fn = call["function"] or {}
        local name = fn.name or "unknown"
        if name ~= "get_weather" then
            return {
                weather_tool_executed = false,
                weather_tool_error = "Unexpected tool: " .. tostring(name),
            }
        end

        local args = json_parse(fn.arguments or "{}")
        local city = "Paris"
        if type(args) == "table" and type(args.city) == "string" and args.city ~= "" then
            city = args.city
        end

        -- Demo/local fallback weather lookup.
        local weather_report = {
            city = city,
            temperature_c = 18,
            condition = "cloudy",
        }

        return {
            weather_tool_executed = true,
            weather_tool_name = name,
            weather_tool_call_id = call.id or "",
            weather_city = city,
            weather_tool_report = weather_report,
        }
    end,
})):depends_on("ask")

flow:step("final", nodes.llm({
    provider = "openai",
    mode = "chat",
    model = "gpt-5-mini",
    output_key = "weather_summary",
    prompt = "Respond with one concise sentence using this tool result only: ${ctx.weather_tool_report}",
})):depends_on("run_tool")

flow:step("print", nodes.log({
    message = "Tool calls: ${ctx.weather_tool_tool_calls}\n"
        .. "Tool executed: ${ctx.weather_tool_executed}\n"
        .. "Final summary: ${ctx.weather_summary_text}",
})):depends_on("final")

return flow
