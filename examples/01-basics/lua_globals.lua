-- Demonstrates Lua globals available in code nodes and function handlers:
-- uuid4(), now_rfc3339(), now_unix_ms(), json_parse(), json_stringify(), log()
local flow = Flow.new("lua_globals_demo")

flow:step("generate_ids", function()
    local id = uuid4()
    local ts = now_rfc3339()
    local ms = now_unix_ms()

    log("info", "Generated ID:", id)
    log("debug", "Timestamp:", ts, "Epoch ms:", ms)

    return {
        request_id = id,
        created_at = ts,
        epoch_ms = ms
    }
end)

flow:step("json_roundtrip", function()
    local data = { name = "Alice", roles = { "admin", "editor" } }

    -- Serialize to JSON string
    local json_str = json_stringify(data)
    log("info", "Serialized:", json_str)

    -- Parse back from JSON string
    local parsed = json_parse('{"score": 42, "active": true}')

    return {
        serialized = json_str,
        parsed_score = parsed.score,
        parsed_active = parsed.active
    }
end)

return flow

-- Run with:
--   ironflow run examples/01-basics/lua_globals.lua --verbose
