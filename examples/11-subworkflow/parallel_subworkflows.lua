-- parallel_subworkflows.lua
-- Execute multiple subworkflows concurrently and collect results.
-- Run: ironflow run examples/11-subworkflow/parallel_subworkflows.lua --verbose

local flow = Flow.new("parallel_demo")

-- Run the greet subworkflow twice in parallel with different inputs
flow:step("run_parallel", nodes.parallel_subworkflows({
    flows = {
        {
            flow = "greet.lua",
            input = { user_name = "Alice" },
            output_key = "greeting_a"
        },
        {
            flow = "greet.lua",
            input = { user_name = "Bob" },
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
