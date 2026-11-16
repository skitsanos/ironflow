-- Lua pattern matching is bounded (IRONFLOW_LUA_MAX_PATTERN_STEPS, default
-- 50000000 steps per Lua state).
--
-- "^.-.-.-.-b" can never match a subject without a "b", but a backtracking
-- matcher tries every way to split the subject between the four lazy repeats
-- first: billions of steps for 1500 characters. The step fails with the
-- budget error instead of holding its run, and the handler reports it. A
-- step timeout or a cancellation would interrupt the same call sooner.
local flow = Flow.new("lua_pattern_budget")

flow:step("linear", function(ctx)
    -- Ordinary matching costs a few steps per character.
    local words = {}
    for word in string.gmatch("the quick brown fox", "%a+") do
        words[#words + 1] = word
    end
    local trimmed = string.gsub("  padded  ", "^%s*(.-)%s*$", "%1")
    return { word_count = #words, trimmed = trimmed }
end)

flow:step("pathological", function(ctx)
    local subject = string.rep("a", 1500)
    return { found = string.find(subject, "^.-.-.-.-b") }
end):depends_on("linear"):on_error("report")

flow:step("report", function(ctx)
    assert(ctx._error_message:find("IRONFLOW_LUA_MAX_PATTERN_STEPS", 1, true), ctx._error_message)
    return { pattern_refused = true }
end)

return flow
