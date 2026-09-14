-- Handler for tool_dispatch_input_projection.lua.
-- Input: payload containing an optional user_id, display_name, and city.

local flow = Flow.new("tool_input_projection_subworkflow")

flow:step("respond", function(ctx)
    assert(ctx.payload.user_id == json_null, "missing ID must not select its parent object")
    assert(ctx.payload.private_note == nil, "unselected fields must stay absent")
    return { tool_result_value = ctx.payload }
end)

return flow
