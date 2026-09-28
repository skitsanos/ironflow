-- Helper for if135_child_error_details.lua. Running it alone succeeds;
-- the parent opts into the intentional failure with should_fail = true.
local flow = Flow.new("if135_error_child")

flow:step("prepare", function()
    return { child_prepared = true }
end)

flow:step("parse", function(ctx)
    if ctx.should_fail == true then
        error("boom: unsupported file type")
    end
    return { parsed = true }
end):depends_on("prepare")

return flow
