-- Demonstrates parallel execution and dependencies
local flow = Flow.new("parallel_demo")

-- These run in parallel from the same context snapshot. Use distinct keys so
-- both outputs remain available after their phase commits.
-- String-valued code is compiled and checked for undefined globals by
-- `ironflow validate`; use `--strict` to reject those warnings.
flow:step("task_a", nodes.code({
    source = "return { task_a_result = 'A complete' }"
}))

flow:step("task_b", nodes.code({
    source = "return { task_b_result = 'B complete' }"
}))

-- This dependency phase sees both committed outputs.
flow:step("merge", nodes.log({
    message = "${ctx.task_a_result}; ${ctx.task_b_result}",
    level = "info"
})):depends_on("task_a", "task_b")

return flow

-- Validate with:
--   ironflow validate examples/01-basics/parallel_execution.lua --strict
