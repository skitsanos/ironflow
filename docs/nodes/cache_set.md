# `cache_set`

Store a value in the cache (memory or file-based) with optional TTL.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `key` | string | yes | — | Cache key to store the value under. Supports `${ctx.*}` interpolation. |
| `source_key` | string | one of `source_key` or `value` | — | Context key whose value will be cached. |
| `value` | any | one of `source_key` or `value` | — | Literal JSON value to cache. |
| `ttl` | integer | no | — | Time-to-live in seconds. When omitted the entry never expires. |
| `backend` | string | no | `"memory"` | Storage backend: `"memory"` (process-global bounded cache) or `"file"` (JSON files on disk). |
| `cache_dir` | string | no | `IRONFLOW_CACHE_DIR` / `".ironflow_cache"` | Directory for file-based cache entries. Only used when `backend` is `"file"`. Per-node value overrides the env var. |

## Context Output

- `cache_key` — the interpolated cache key that was written.
- `cache_stored` — always `true` on success.
- `cache_size` — current memory cache entry count; only returned for the `"memory"` backend.

`cache_set` and `cache_get` apply the same interpolation rules to `key`, so wrappers can compute a key once from context and use the same expression for both write and read. For example, `llm:${ctx.prompt_hash}` writes and reads the concrete key `llm:<hash>`.

## Example

### Memory backend

```lua
local flow = Flow.new("cache_to_memory")

flow:step("store", nodes.cache_set({
    key = "user_token:${ctx.user_id}",
    source_key = "auth_response",
    ttl = 3600,
    backend = "memory"
}))

flow:step("done", nodes.log({
    message = "Cached key: ${ctx.cache_key}, stored: ${ctx.cache_stored}"
})):depends_on("store")

return flow
```

## Environment

- `IRONFLOW_CACHE_MAX_ENTRIES` controls the process-global memory backend size. Default: `10000`.
- `IRONFLOW_CACHE_DIR` controls the default file backend directory when `cache_dir` is not set. Default: `.ironflow_cache`.

### File backend

```lua
local flow = Flow.new("cache_to_file")

flow:step("store", nodes.cache_set({
    key = "report:${ctx.report_id}",
    value = { status = "complete", score = 95 },
    ttl = 86400,
    backend = "file",
    cache_dir = "/tmp/my_cache"
}))

flow:step("done", nodes.log({
    message = "Persisted key: ${ctx.cache_key}"
})):depends_on("store")

return flow
```
