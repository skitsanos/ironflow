-- File-backed cache retained on disk for inspection after one flow run.
-- Each execution chooses a new UUID-scoped directory and does not reuse entries
-- from an earlier run.
-- Punctuation stays part of the key. Records live in v1/<sha256>.json and
-- retain the original key for read verification; legacy files are cache misses.
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
    key = "app:config/main",
    value = { version = "1.1.0", debug = false, max_retries = 3 },
    backend = "file",
    cache_dir = cache_dir,
    ttl = 86400
}))

-- This key previously collided with app:config/main after sanitization.
flow:step("save_alternate", nodes.cache_set({
    key = "app:config?main",
    value = { version = "2.0.0", debug = true, max_retries = 7 },
    backend = "file",
    cache_dir = cache_dir,
    ttl = 86400
})):depends_on("save_config")

-- Read it back
flow:step("load_config", nodes.cache_get({
    key = "app:config/main",
    output_key = "config",
    backend = "file",
    cache_dir = cache_dir
})):depends_on("save_alternate")

flow:step("load_alternate", nodes.cache_get({
    key = "app:config?main",
    output_key = "alternate_config",
    backend = "file",
    cache_dir = cache_dir
})):depends_on("load_config")

flow:step("verify", function(ctx)
    assert(ctx.config.version == "1.1.0", "Main cache key was overwritten")
    assert(ctx.alternate_config.version == "2.0.0", "Alternate cache key was overwritten")
    return { cache_identity_verified = true }
end):depends_on("load_alternate")

flow:step("done", nodes.log({
    message = "Loaded config, cache hit: ${ctx.cache_hit}"
})):depends_on("verify")

return flow
