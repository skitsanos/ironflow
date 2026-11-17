# `cache_get`

Read a value stored by [`cache_set`](cache_set.md) from the memory or file cache.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `key` | string | Yes | — | Cache key; supports `${ctx.key}` interpolation, the same way as `cache_set`. |
| `output_key` | string | No | `"cached_value"` | Context key for the retrieved value. |
| `backend` | string | No | `"memory"` | `"memory"` (process-global) or `"file"` (JSON files on disk). |
| `cache_dir` | string | No | `IRONFLOW_CACHE_DIR` or `".ironflow_cache"` | Directory for the file backend; ignored by the memory backend. |

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}` | any | The cached value, or `null` on a miss or expired entry. |
| `cache_hit` | boolean | `true` when a non-expired entry was found. |

## Behavior

- A miss is not an error; check `cache_hit`.
- Expired entries are removed when read.
- The memory cache is shared by all runs in one IronFlow process and is lost on restart.
- An unknown `backend` fails the step.

## File Identity and Upgrades

- Each entry is stored at `<cache_dir>/v1/<sha256>.json`, where the digest is SHA-256 of the interpolated key's exact UTF-8 bytes. Keys are not sanitized, so `report:a/b` and `report:a?b` are different entries.
- A record holds `schema_version: 1`, the original `key`, `value`, and optional `expires_at`. A record whose version or stored key does not match is a miss and is left untouched.
- A malformed record fails the step instead of returning a miss.
- Root-level `<sanitized_key>.json` files from older releases are not read, migrated or deleted; the first lookup after upgrading is a miss.
- Keys and values are plaintext inside records; keep the cache directory access-controlled.

## Limits

| Variable | Default | Effect |
|----------|---------|--------|
| `IRONFLOW_CACHE_MAX_ENTRIES` | `10000` | Maximum entries in the memory cache; the least recently used entry is evicted. |
| `IRONFLOW_CACHE_DIR` | `.ironflow_cache` | Default `cache_dir` for the file backend. |

## Examples

### Memory backend

```lua
local flow = Flow.new("read_from_memory_cache")

flow:step("lookup", nodes.cache_get({
    key = "user_token:${ctx.user_id}",
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
