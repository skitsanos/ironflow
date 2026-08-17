--[[
Tool dispatch flow using nodes.tool_dispatch as a first-class tool handler.

Flow:
1) Call nodes.llm with tool definitions for `get_weather` and `get_time`.
2) Dispatch every returned tool call to a mapped subworkflow.
3) Build the assistant/tool continuation message array in workflow context.
4) Use messages_key for the final model turn.

Requirements:
- Network access and OPENAI_API_KEY.
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

flow:step("run_tools", nodes.tool_dispatch({
    source_key = "llm_tool_request_tool_calls_normalized",
    output_key = "tool_results",
    tools = {
        get_weather = {
            flow = "tool_weather_subworkflow.lua",
            input = {
                city = "arguments.city",
            },
        },
    },
})):depends_on("ask")

flow:step("build_followup", function(ctx)
    local messages = {
        {
            role = "user",
            content = "What is the weather in Berlin?",
        },
        ctx.llm_tool_request_raw.choices[1].message,
    }
    for _, message in ipairs(ctx.tool_results_messages or {}) do
        table.insert(messages, message)
    end
    return { tool_conversation = messages }
end):depends_on("run_tools")

flow:step("final", nodes.llm({
    provider = "openai",
    mode = "chat",
    model = "gpt-5-mini",
    output_key = "llm_tool_final",
    messages_key = "tool_conversation",
})):depends_on("build_followup")

flow:step("print", nodes.log({
    message = "Tool calls: ${ctx.llm_tool_request_tool_calls_normalized}\n"
        .. "Tool results: ${ctx.tool_results}\n"
        .. "Final: ${ctx.llm_tool_final_text}",
})):depends_on("final")

return flow
