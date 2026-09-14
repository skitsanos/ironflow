# `cache_get`

Retrieve a value from the cache (memory or file-based).

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `key` | string | yes | — | Cache key to look up. Supports `${ctx.key}` interpolation. |
| `output_key` | string | no | `"cached_value"` | Context key where the retrieved value is stored. |
| `backend` | string | no | `"memory"` | Storage backend: `"memory"` (process-global bounded cache) or `"file"` (JSON files on disk). |
| `cache_dir` | string | no | `IRONFLOW_CACHE_DIR` / `".ironflow_cache"` | Directory for file-based cache entries. Only used when `backend` is `"file"`. Per-node value overrides the env var. |

> Expired memory entries and verified expired file entries are removed on access.

`cache_get` and `cache_set` apply the same interpolation rules to `key`, so a value stored with `key = "llm:${ctx.prompt_hash}"` can be read back with the same expression. The interpolated key is also returned by `cache_set` as `cache_key`.

## File Identity and Upgrades

File entries use `<cache_dir>/v1/<sha256>.json`, where the lowercase hex digest
is SHA-256 of the interpolated key's exact UTF-8 bytes. Punctuation, case,
Unicode normalization forms, and path-like text remain distinct; the key is not
sanitized or interpreted as a filesystem path.

Each JSON record contains `schema_version: 1`, the original `key`, `value`, and
optional `expires_at`. A read verifies the version and exact key before returning
a hit or cleaning up an expired file. Missing/mismatched identity or a missing/
unsupported version produces a miss and leaves the record untouched. Malformed
JSON or incompatible field types remain errors rather than hits.

Legacy root-level `<sanitized_key>.json` files are **not read, migrated, or
deleted**. Their original keys are ambiguous, so upgrading causes a cache miss
until the workflow repopulates the versioned cache. Legacy files can be removed
separately after they are no longer needed by older binaries.

The file backend is a best-effort cache, not a transactional state store. File
work uses tracked blocking workers, with cancellation/deadline checks before
and after I/O; an in-progress synchronous operation cannot be preempted or
rolled back. Concurrent file writes and expiry cleanup, including from other
processes, are not coordinated. Keep the cache directory trusted and
access-controlled: keys and values remain plaintext inside records, and the
digest is not authentication or encryption.

## Context Output

- `<output_key>` (default `cached_value`) — the cached value, or `null` if not found / expired.
- `cache_hit` — `true` if a valid (non-expired) entry was found, `false` otherwise.

## Example

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

## Environment

- `IRONFLOW_CACHE_MAX_ENTRIES` controls the process-global memory backend size. Default: `10000`.
- `IRONFLOW_CACHE_DIR` controls the default file backend directory when `cache_dir` is not set. Default: `.ironflow_cache`.

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
