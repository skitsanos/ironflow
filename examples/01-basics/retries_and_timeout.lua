-- Demonstrates retry and timeout configuration
local flow = Flow.new("retries_demo")

-- This step will timeout after 2 seconds
flow:step("slow_task", nodes.delay({
    seconds = 5
})):timeout(2)

-- This step has retry logic (3 attempts, 0.5s initial backoff)
-- It depends on slow_task, which will fail due to timeout,
-- so this step will be skipped
flow:step("after_slow", nodes.log({
    message = "This won't run because slow_task timed out",
    level = "info"
})):depends_on("slow_task"):retries(3, 0.5)

-- Independent step — runs regardless of slow_task's failure
flow:step("independent", nodes.log({
    message = "I run independently!",
    level = "info"
}))

return flow

-- Run with:
--   ironflow run examples/01-basics/retries_and_timeout.lua --verbose
