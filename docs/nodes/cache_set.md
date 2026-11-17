# `cache_set`

Store a value in the memory or file cache with an optional time-to-live.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `key` | string | Yes | — | Cache key; supports `${ctx.key}` interpolation, the same way as `cache_get`. |
| `source_key` | string | One of `source_key` or `value` | — | Context key whose value is cached; takes precedence over `value`. |
| `value` | any | One of `source_key` or `value` | — | Literal value to cache. |
| `ttl` | integer | No | — | Time-to-live in seconds; omitted means the entry never expires, `0` expires it immediately. |
| `backend` | string | No | `"memory"` | `"memory"` (process-global) or `"file"` (JSON files on disk). |
| `cache_dir` | string | No | `IRONFLOW_CACHE_DIR` or `".ironflow_cache"` | Directory for the file backend; ignored by the memory backend. |

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `cache_key` | string | The interpolated key that was written. |
| `cache_stored` | boolean | `true` on success. |
| `cache_size` | number | Entries in the memory cache; memory backend only. |

## Behavior

- A missing `source_key` in context, or neither `source_key` nor `value`, fails the step.
- Writing an existing key replaces its value and TTL.
- The memory cache is shared by all runs in one IronFlow process and is lost on restart.
- The file backend writes each record atomically, so a concurrent `cache_get` never reads a partial record. Concurrent writes of one key are last-writer-wins.
- A symlink or non-regular file at the entry path fails the step.
- File layout and upgrade behavior are described in [`cache_get`](cache_get.md#file-identity-and-upgrades). Missing directories are created as described in [Created directories](write_file.md#created-directories).

## Limits

| Variable | Default | Effect |
|----------|---------|--------|
| `IRONFLOW_CACHE_MAX_ENTRIES` | `10000` | Maximum entries in the memory cache; the least recently used entry is evicted. |
| `IRONFLOW_CACHE_DIR` | `.ironflow_cache` | Default `cache_dir` for the file backend. |

## Examples

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
