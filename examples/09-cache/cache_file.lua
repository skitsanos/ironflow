-- File-backed cache retained on disk for inspection after one flow run.
-- Each execution chooses a new UUID-scoped directory and does not reuse entries
-- from an earlier run.
-- Effects: retains a UUID-scoped cache directory under TMPDIR, TMP, TEMP, or
-- `.` so the on-disk entry can be inspected after the run.
local flow = Flow.new("cache_file_demo")
local temp_root = env("TMPDIR")
if temp_root == nil or temp_root == "" then temp_root = env("TMP") end
if temp_root == nil or temp_root == "" then temp_root = env("TEMP") end
if temp_root == nil or temp_root == "" then temp_root = "." end
local cache_dir = temp_root .. "/ironflow-cache-file-" .. uuid4()

-- Store a config value to disk
flow:step("save_config", nodes.cache_set({
    key = "app_config",
    value = { version = "1.1.0", debug = false, max_retries = 3 },
    backend = "file",
    cache_dir = cache_dir,
    ttl = 86400
}))

-- Read it back
flow:step("load_config", nodes.cache_get({
    key = "app_config",
    output_key = "config",
    backend = "file",
    cache_dir = cache_dir
})):depends_on("save_config")

flow:step("done", nodes.log({
    message = "Loaded config, cache hit: ${ctx.cache_hit}"
})):depends_on("load_config")

return flow
