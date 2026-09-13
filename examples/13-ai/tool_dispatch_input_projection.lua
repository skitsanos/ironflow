-- Offline tool dispatch with explicit nested parent-context selection.
-- The private field is synthetic and remains in parent history. It must not
-- enter the selected child payload or the tool results sent to a model.

local flow = Flow.new("tool_dispatch_input_projection")

flow:step("seed", function()
    return {
        user = {
            profile = { name = "Ada" },
            private_note = "synthetic-private-field",
        },
        calls = {
            {
                id = "call-1",
                index = 0,
                type = "function",
                name = "project_user",
                arguments = { city = "Berlin" },
                raw_arguments = '{"city":"Berlin"}',
            },
        },
    }
end)

flow:step("dispatch", nodes.tool_dispatch({
    source_key = "calls",
    tools = {
        project_user = {
            flow = "tool_input_projection_subworkflow.lua",
            input = {
                payload = {
                    user_id = "ctx.user.id",
                    display_name = "ctx.user.profile.name",
                    city = "arguments.city",
                },
            },
        },
    },
})):depends_on("seed")

flow:step("verify", function(ctx)
    local result = ctx.tool_results[1].result
    assert(ctx.tool_results_all_succeeded, "tool handler must succeed")
    assert(result.user_id == json_null, "missing user ID must be JSON null")
    assert(result.display_name == "Ada", "nested name must be preserved")
    assert(result.city == "Berlin", "tool argument must be preserved")
    local output = json_stringify({
        results = ctx.tool_results,
        messages = ctx.tool_results_messages,
        by_id = ctx.tool_results_by_id,
    })
    assert(not output:find("synthetic-private-field", 1, true), "private sibling reached tool output")
    return { projection_verified = true }
end):depends_on("dispatch")

return flow
