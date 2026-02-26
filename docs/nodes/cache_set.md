# `cache_set`

Store a value in the cache (memory or file-based) with optional TTL.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `key` | string | yes | — | Cache key to store the value under. |
| `source_key` | string | one of `source_key` or `value` | — | Context key whose value will be cached. |
| `value` | any | one of `source_key` or `value` | — | Literal JSON value to cache. |
| `ttl` | integer | no | — | Time-to-live in seconds. When omitted the entry never expires. |
| `backend` | string | no | `"memory"` | Storage backend: `"memory"` (in-process global HashMap) or `"file"` (JSON files on disk). |
| `cache_dir` | string | no | `".ironflow_cache"` | Directory for file-based cache entries. Only used when `backend` is `"file"`. |

## Context Output

- `cache_key` — the cache key that was written.
- `cache_stored` — always `true` on success.

## Example

### Memory backend

```lua
local flow = Flow.new("cache_to_memory")

flow:step("store", nodes.cache_set({
    key = "user_token",
    source_key = "auth_response",
    ttl = 3600,
    backend = "memory"
}))

flow:step("done", nodes.log({
    message = "Cached key: ${ctx.cache_key}, stored: ${ctx.cache_stored}"
})):depends_on("store")

return flow
```

### File backend

```lua
local flow = Flow.new("cache_to_file")

flow:step("store", nodes.cache_set({
    key = "report_data",
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
