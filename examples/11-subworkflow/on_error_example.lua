-- on_error_example.lua — Demonstrate per-step error handling

local flow = Flow.new("on_error_demo")
local temp_root = env("TMPDIR")
if temp_root == nil or temp_root == "" then temp_root = env("TMP") end
if temp_root == nil or temp_root == "" then temp_root = env("TEMP") end
if temp_root == nil or temp_root == "" then temp_root = "." end
local missing_path = temp_root .. "/ironflow-missing-" .. uuid4() .. ".txt"

-- Recovery handlers are planned DAG work and may declare normal dependencies.
flow:step("prepare_recovery", nodes.code({
    source = function(ctx)
        return { recovery_channel = "workflow-log" }
    end
}))

-- This step will fail (accessing a non-existent file)
flow:step("risky_step", nodes.read_file({
    path = missing_path
})):on_error("handle_error")

-- The dedicated handler receives invocation-local _error_message,
-- _error_step, and _error_node_type. Returning error_info persists only the
-- selected recovery result, not the private metadata itself.
flow:step("handle_error", nodes.code({
    source = function(ctx)
        return {
            error_handled = true,
            error_info = "[" .. ctx.recovery_channel .. "] Caught error in step '" .. (ctx._error_step or "?") .. "': " .. (ctx._error_message or "unknown")
        }
    end
})):depends_on("prepare_recovery")

-- This is a recovery-only branch. If risky_step succeeds, handle_error and
-- final are skipped without failing the run.
flow:step("final", nodes.log({
    message = "Error was handled: ${ctx.error_info}",
    level = "info"
})):depends_on("handle_error")

return flow

-- Run with:
--   ironflow run examples/11-subworkflow/on_error_example.lua
