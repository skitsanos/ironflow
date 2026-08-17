-- Repeat one child workflow with explicit carried state and a finite bound.
-- The child runs sequentially until it returns repeat_done = true.
local flow = Flow.new("repeat_subworkflow")

flow:step("initialize", function()
    return {
        initial_counter = { value = 0 },
        target = 3
    }
end)

flow:step("count", nodes.repeat_subworkflow({
    flow = "repeat_counter_subworkflow.lua",
    input = {
        repeat_state = "initial_counter",
        target = "target"
    },
    max_iterations = 5,
    output_key = "counter"
})):depends_on("initialize")

flow:step("report", nodes.log({
    message = "Counter reached ${ctx.counter_state.value} after ${ctx.counter_iterations} iterations"
})):depends_on("count")

return flow
