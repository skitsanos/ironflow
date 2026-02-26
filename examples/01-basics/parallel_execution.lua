-- Demonstrates parallel execution and dependencies
local flow = Flow.new("parallel_demo")

-- These two run in parallel (no dependencies)
flow:step("task_a", nodes.log({
    message = "Task A running...",
    level = "info"
}))

flow:step("task_b", nodes.delay({
    seconds = 1
}))

-- This runs after both A and B complete
flow:step("merge", nodes.log({
    message = "Both tasks complete!",
    level = "info"
})):depends_on("task_a", "task_b")

return flow
