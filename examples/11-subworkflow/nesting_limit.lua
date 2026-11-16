-- A flow that calls itself is stopped by the child workflow nesting limit
-- (IRONFLOW_MAX_SUBWORKFLOW_DEPTH, default 16) instead of descending forever.
--
-- Every level starts the same flow again. The level at the ceiling is refused
-- before a seventeenth child level is loaded, so its run fails; the level
-- above tolerates that failure with on_error = "ignore" and reports it, and
-- the report travels back up to the top-level run.
local flow = Flow.new("nesting_limit")

flow:step("descend", nodes.subworkflow({
    flow = "nesting_limit.lua",
    input = {},
    on_error = "ignore"
}))

flow:step("report", function(ctx)
    if type(ctx.subworkflow_error) == "string" then
        -- This level's child was the one refused at the ceiling.
        assert(ctx.subworkflow_error:find("child workflow nesting depth 17 exceeds the limit of 16", 1, true),
            ctx.subworkflow_error)
        return { nesting_refusal = ctx.subworkflow_error, levels_above_refusal = 0 }
    end
    assert(type(ctx.nesting_refusal) == "string", "the refusal did not reach this level")
    return { levels_above_refusal = ctx.levels_above_refusal + 1 }
end):depends_on("descend")

return flow
