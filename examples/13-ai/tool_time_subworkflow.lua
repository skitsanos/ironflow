--[[
Subworkflow: time tool execution.

Input:
- city (string): City requested by the dispatcher.

Output:
- tool_result_name: Identifier for the produced result.
- tool_result_text: Human-readable result message.
- tool_result_value: Structured tool output object.
]]

local flow = Flow.new("tool_time_subworkflow")

flow:step("time_tool", nodes.code({
    source = function(ctx)
        local city = ctx.city
        if type(city) ~= "string" or city == "" then
            city = "Paris"
        end

        local timestamp = os.date("!%Y-%m-%d %H:%M:%S UTC")
        return {
            tool_result_name = "time",
            tool_result_text = "Current time in " .. city .. " is " .. timestamp .. ".",
            tool_result_value = {
                city = city,
                timestamp = timestamp,
                source = "subworkflow"
            },
        }
    end,
} ))

return flow

