--[[
Subworkflow: unknown tool fallback.

Input:
- tool_name (string): Tool name that was requested but has no implementation.
- tool_raw_arguments (string): Raw tool arguments from the LLM.

Output:
- tool_result_name: Identifier for the produced result.
- tool_result_text: Human-readable result message.
- tool_result_value: Structured tool output object.
]]

local flow = Flow.new("tool_unknown_subworkflow")

flow:step("fallback", nodes.code({
    source = function(ctx)
        local tool_name = ctx.tool_name or "unknown"
        local tool_args = ctx.tool_raw_arguments or "{}"

        return {
            tool_result_name = "fallback",
            tool_result_text = "No handler is available for tool '" .. tostring(tool_name) .. "'.",
            tool_result_value = {
                tool = tool_name,
                arguments = tool_args,
            },
        }
    end,
} ))

return flow

