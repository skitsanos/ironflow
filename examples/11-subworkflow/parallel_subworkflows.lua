-- parallel_subworkflows.lua
-- Execute multiple subworkflows concurrently and collect results.
-- Run: ironflow run examples/11-subworkflow/parallel_subworkflows.lua --verbose

local flow = Flow.new("parallel_demo")

-- Run the greet subworkflow twice in parallel with different inputs.
-- A plain string in `input` names a parent context key that must exist;
-- `{ literal = ... }` passes the text itself.
flow:step("run_parallel", nodes.parallel_subworkflows({
    flows = {
        {
            flow = "greet.lua",
            input = { name = { literal = "Alice" } },
            output_key = "greeting_a"
        },
        {
            flow = "greet.lua",
            input = { name = { literal = "Bob" } },
            output_key = "greeting_b"
        }
    },
    on_error = "ignore"
}))

flow:step("summary", function(ctx)
    return {
        total = ctx.parallel_results_count,
        all_ok = ctx.parallel_results_all_succeeded
    }
end):depends_on("run_parallel")

return flow
