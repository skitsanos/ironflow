-- File-based cache: persists across flow runs
local flow = Flow.new("cache_file_demo")

-- Store a config value to disk
flow:step("save_config", nodes.cache_set({
    key = "app_config",
    value = { version = "1.1.0", debug = false, max_retries = 3 },
    backend = "file",
    ttl = 86400
}))

-- Read it back
flow:step("load_config", nodes.cache_get({
    key = "app_config",
    output_key = "config",
    backend = "file"
})):depends_on("save_config")

flow:step("done", nodes.log({
    message = "Loaded config, cache hit: ${ctx.cache_hit}"
})):depends_on("load_config")

return flow
