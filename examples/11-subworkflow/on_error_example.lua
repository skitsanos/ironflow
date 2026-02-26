-- on_error_example.lua — Demonstrate per-step error handling

local flow = Flow.new("on_error_demo")

-- This step will fail (accessing a non-existent file)
flow:step("risky_step", nodes.read_file({
    path = "/tmp/nonexistent_file_abc123.txt"
})):on_error("handle_error")

-- Error handler receives _error_message, _error_step, _error_node_type
flow:step("handle_error", nodes.code({
    source = function(ctx)
        return {
            error_handled = true,
            error_info = "Caught error in step '" .. (ctx._error_step or "?") .. "': " .. (ctx._error_message or "unknown")
        }
    end
}))

flow:step("final", nodes.log({
    message = "Error was handled: ${ctx.error_info}",
    level = "info"
})):depends_on("handle_error")

return flow

-- Run with:
--   ironflow run examples/11-subworkflow/on_error_example.lua
