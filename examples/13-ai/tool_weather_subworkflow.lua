--[[
Subworkflow: weather tool execution.

Input:
- city (string): City requested by the dispatcher.

Output:
- tool_result_name: Identifier for the produced result.
- tool_result_text: Human-readable result message.
- tool_result_value: Structured tool output object.
]]

local flow = Flow.new("tool_weather_subworkflow")

flow:step("weather_tool", nodes.code({
    source = function(ctx)
        local city = ctx.city
        if type(city) ~= "string" or city == "" then
            city = "Paris"
        end

        return {
            tool_result_name = "weather",
            tool_result_text = "Weather fetched for " .. city .. ": mostly sunny, 21°C.",
            tool_result_value = {
                city = city,
                temperature_c = 21,
                condition = "mostly sunny",
            },
        }
    end,
} ))

return flow

