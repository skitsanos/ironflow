-- Cache keys can be derived from context in both cache_set and cache_get.
-- Effects: retains a UUID-scoped cache directory under TMPDIR, TMP, TEMP, or
-- `.` so the interpolated file-backed entry can be inspected after the run.

local flow = Flow.new("cache_context_keys")
local temp_root = env("TMPDIR")
if temp_root == nil or temp_root == "" then temp_root = env("TMP") end
if temp_root == nil or temp_root == "" then temp_root = env("TEMP") end
if temp_root == nil or temp_root == "" then temp_root = "." end
local cache_dir = temp_root .. "/ironflow-cache-context-" .. uuid4()

flow:step("seed", nodes.code({
    source = function(ctx)
        local user_id = ctx.user_id
        if user_id == nil then user_id = "u-1001" end
        local prompt_hash = ctx.prompt_hash
        if prompt_hash == nil then prompt_hash = "demo-hash" end
        return {
            user_id = user_id,
            prompt_hash = prompt_hash,
            user_token = "token-for-" .. tostring(user_id),
            llm_response = {
                model = "demo",
                text = "cached response"
            }
        }
    end
}))

flow:step("store_memory", nodes.cache_set({
    key = "user:${ctx.user_id}:token",
    source_key = "user_token",
    ttl = 3600,
    backend = "memory"
})):depends_on("seed")

flow:step("store_file", nodes.cache_set({
    key = "llm:${ctx.prompt_hash}",
    source_key = "llm_response",
    ttl = 86400,
    backend = "file",
    cache_dir = cache_dir
})):depends_on("seed", "store_memory")

flow:step("load_memory", nodes.cache_get({
    key = "user:${ctx.user_id}:token",
    output_key = "cached_token",
    backend = "memory"
})):depends_on("store_memory")

flow:step("load_file", nodes.cache_get({
    key = "llm:${ctx.prompt_hash}",
    output_key = "cached_llm_response",
    backend = "file",
    cache_dir = cache_dir
})):depends_on("store_file")

flow:step("done", nodes.log({
    message = "Loaded interpolated cache keys: token=${ctx.cached_token}, llm_hit=${ctx.cache_hit}"
})):depends_on("load_memory", "load_file")

return flow
