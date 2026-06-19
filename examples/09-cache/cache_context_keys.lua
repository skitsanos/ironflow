-- Cache keys can be derived from context in both cache_set and cache_get.

local flow = Flow.new("cache_context_keys")

flow:step("seed", nodes.code({
    source = function(ctx)
        return {
            user_id = ctx.user_id or "u-1001",
            prompt_hash = ctx.prompt_hash or "demo-hash",
            user_token = "token-for-" .. (ctx.user_id or "u-1001"),
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
    backend = "file"
})):depends_on("seed")

flow:step("load_memory", nodes.cache_get({
    key = "user:${ctx.user_id}:token",
    output_key = "cached_token",
    backend = "memory"
})):depends_on("store_memory")

flow:step("load_file", nodes.cache_get({
    key = "llm:${ctx.prompt_hash}",
    output_key = "cached_llm_response",
    backend = "file"
})):depends_on("store_file")

flow:step("done", nodes.log({
    message = "Loaded interpolated cache keys: token=${ctx.cached_token}, llm_hit=${ctx.cache_hit}"
})):depends_on("load_memory", "load_file")

return flow
