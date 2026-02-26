-- In-memory cache: store and retrieve values within the same flow run
local flow = Flow.new("cache_memory_demo")

-- Store a value in memory cache
flow:step("store_token", nodes.cache_set({
    key = "auth_token",
    value = "sk-abc123-secret",
    ttl = 3600
}))

-- Store a context value
flow:step("setup_data", nodes.code({
    source = [[
        return { user = { id = 42, name = "Alice", role = "admin" } }
    ]]
}))

flow:step("cache_user", nodes.cache_set({
    key = "current_user",
    source_key = "user"
})):depends_on("setup_data")

-- Retrieve from cache
flow:step("get_token", nodes.cache_get({
    key = "auth_token",
    output_key = "cached_token"
})):depends_on("store_token")

flow:step("get_user", nodes.cache_get({
    key = "current_user",
    output_key = "cached_user"
})):depends_on("cache_user")

-- Try a missing key
flow:step("get_missing", nodes.cache_get({
    key = "nonexistent",
    output_key = "missing_value"
})):depends_on("store_token")

-- Log results
flow:step("done", nodes.log({
    message = "Token hit: ${ctx.cache_hit}"
})):depends_on("get_token", "get_user", "get_missing")

return flow
