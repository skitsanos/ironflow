# `cache_get`

Retrieve a value from the cache (memory or file-based).

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `key` | string | yes | — | Cache key to look up. Supports `${ctx.*}` interpolation. |
| `output_key` | string | no | `"cached_value"` | Context key where the retrieved value is stored. |
| `backend` | string | no | `"memory"` | Storage backend: `"memory"` (in-process global HashMap) or `"file"` (JSON files on disk). |
| `cache_dir` | string | no | `".ironflow_cache"` | Directory for file-based cache entries. Only used when `backend` is `"file"`. |

> Expired entries are automatically removed on access (from both memory and file backends).

## Context Output

- `<output_key>` (default `cached_value`) — the cached value, or `null` if not found / expired.
- `cache_hit` — `true` if a valid (non-expired) entry was found, `false` otherwise.

## Example

### Memory backend

```lua
local flow = Flow.new("read_from_memory_cache")

flow:step("lookup", nodes.cache_get({
    key = "user_token",
    output_key = "token",
    backend = "memory"
}))

flow:step("done", nodes.log({
    message = "Hit: ${ctx.cache_hit}, Value: ${ctx.token}"
})):depends_on("lookup")

return flow
```

### File backend

```lua
local flow = Flow.new("read_from_file_cache")

flow:step("lookup", nodes.cache_get({
    key = "${ctx.user_id}_token",
    output_key = "token",
    backend = "file",
    cache_dir = "/tmp/my_cache"
}))

flow:step("done", nodes.log({
    message = "Hit: ${ctx.cache_hit}, Value: ${ctx.token}"
})):depends_on("lookup")

return flow
```
