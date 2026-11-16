-- Demonstrates guard comparisons against true, false and nil.
--
-- The right side of a comparison is always a literal: a number, a quoted
-- string, true, false or nil. `!=` is the exact negation of `==`.
local flow = Flow.new("guard_literals_demo")

flow:step("prepare", nodes.code({
    source = [[
        return {
            ready = true,
            dry_run = false,
            attempts = 2,
            mode = "full",
        }
    ]]
}))

-- Boolean literals
flow:step_if("ctx.ready == true", "start", nodes.log({
    message = "ready is true"
})):depends_on("prepare")

flow:step_if("ctx.dry_run == false", "write", nodes.log({
    message = "dry_run is false, so the write happens"
})):depends_on("prepare")

flow:step_if("ctx.dry_run != false", "preview", nodes.log({
    message = "skipped: dry_run is false"
})):depends_on("prepare")

-- nil matches a key that is absent or null
flow:step_if("ctx.override == nil", "use_default", nodes.log({
    message = "no override in context, using the default"
})):depends_on("prepare")

flow:step_if("ctx.override != nil", "use_override", nodes.log({
    message = "skipped: there is no override"
})):depends_on("prepare")

-- Numbers and strings work as before
flow:step_if("ctx.attempts >= 2", "escalate", nodes.log({
    message = "attempts: ${ctx.attempts}"
})):depends_on("prepare")

-- The same grammar applies to if_node with named routes
flow:step("check_mode", nodes.if_node({
    condition = "ctx.mode != 'full'",
    true_route = "partial",
    false_route = "full"
})):depends_on("prepare")

flow:step("full_run", nodes.log({
    message = "mode is full"
})):depends_on("check_mode"):route("full")

flow:step("partial_run", nodes.log({
    message = "skipped: mode is full"
})):depends_on("check_mode"):route("partial")

return flow

-- Run with:
--   ironflow run examples/03-control-flow/guard_literals.lua
--
-- A comparison with no defined result is an error, not a skipped step:
--   "ctx.attempts > nil"   is rejected when the flow is loaded
--   "ctx.ready > 0"        fails the guard step (a boolean cannot be ordered)
