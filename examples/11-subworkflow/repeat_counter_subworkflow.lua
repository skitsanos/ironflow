-- Reusable child for repeat_subworkflow.lua. The parent supplies repeat_state,
-- repeat_iteration, and target; this child publishes the explicit next state
-- and completion decision required by repeat_subworkflow.
local flow = Flow.new("repeat_counter_subworkflow")

flow:step("advance", function(ctx)
    local current = ctx.repeat_state or { value = 0 }
    local next_state = { value = current.value + 1 }

    return {
        repeat_next_state = next_state,
        repeat_done = next_state.value >= ctx.target,
        current_value = next_state.value,
        observed_iteration = ctx.repeat_iteration
    }
end)

return flow
