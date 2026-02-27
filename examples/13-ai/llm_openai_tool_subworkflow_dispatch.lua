--[[
Tool dispatch flow using nodes.subworkflow as tool handlers.

Flow:
1) Call nodes.llm with tool definitions for `get_weather` and `get_time`.
2) Parse the first tool call into a normalized dispatch context.
3) Route to a matching subworkflow via switch_node.
4) Run the selected subworkflow (`tool_weather_subworkflow`, `tool_time_subworkflow`,
   or fallback).
5) Use the resulting structured tool payload to produce a user-facing summary.
]]

local flow = Flow.new("llm_openai_tool_subworkflow_dispatch")

flow:step("ask", nodes.llm({
    provider = "openai",
    mode = "chat",
    model = "gpt-5-mini",
    output_key = "llm_tool_request",
    messages = {
        {
            role = "user",
            content = "What is the weather in Berlin?",
        },
    },
    tools = {
        {
            type = "function",
            ["function"] = {
                name = "get_weather",
                description = "Get the weather in a city.",
                parameters = {
                    type = "object",
                    properties = {
                        city = { type = "string", description = "City name" },
                    },
                    required = { "city" },
                    additionalProperties = false,
                },
            },
        },
    },
    tool_choice = "required",
}))

flow:step("dispatch", nodes.code({
    source = function(ctx)
        local calls = ctx.llm_tool_request_tool_calls
        if type(calls) ~= "table" or #calls == 0 then
            return {
                tool_dispatch_name = "unsupported",
                tool_dispatch_city = "Paris",
                tool_raw_arguments = "{}",
            }
        end

        local first = calls[1]
        local fn = first["function"] or {}
        local name = fn.name or "unsupported"
        local args_raw = fn.arguments or "{}"
        local args = json_parse(args_raw)

        local city = "Paris"
        if type(args) == "table" and type(args.city) == "string" and args.city ~= "" then
            city = args.city
        end

        return {
            tool_dispatch_name = name,
            tool_dispatch_city = city,
            tool_raw_arguments = args_raw,
        }
    end,
})):depends_on("ask")

flow:step("dispatch_by_tool", nodes.switch_node({
    value = "ctx.tool_dispatch_name",
    cases = {
        get_weather = "weather",
    },
    default = "unsupported",
})):depends_on("dispatch")

flow:step("run_weather_tool", nodes.subworkflow({
    flow = "tool_weather_subworkflow.lua",
    input = {
        city = "tool_dispatch_city",
    },
})):depends_on("dispatch_by_tool"):route("weather")

flow:step("run_unsupported_tool", nodes.subworkflow({
    flow = "tool_unknown_subworkflow.lua",
    input = {
        tool_name = "tool_dispatch_name",
        tool_raw_arguments = "tool_raw_arguments",
    },
})):depends_on("dispatch_by_tool"):route("unsupported")

flow:step("final", nodes.llm({
    provider = "openai",
    mode = "chat",
    model = "gpt-5-mini",
    output_key = "llm_tool_final",
    prompt = "Use this tool output to respond in one sentence: ${ctx.tool_result_text}\nTool payload: ${ctx.tool_result_value}",
})):depends_on("run_weather_tool", "run_unsupported_tool")

flow:step("print", nodes.log({
    message = "Tool dispatch: ${ctx.tool_dispatch_name}, city=${ctx.tool_dispatch_city}, result=${ctx.tool_result_text}",
})):depends_on("final")

return flow
