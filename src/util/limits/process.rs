//! Process-wide capacities that nodes read once into a `OnceLock` or
//! `LazyLock`. Startup validation is what makes a typo visible for these: by
//! the time a node reads one, the value is already baked into the process.

use super::Limit;

/// Detached `subworkflow` executions admitted at once.
pub(crate) const MAX_DETACHED_SUBWORKFLOWS: Limit =
    Limit::positive("IRONFLOW_MAX_DETACHED_SUBWORKFLOWS", 64);
/// Distinct OAuth client tuples remembered across the process.
pub(crate) const OAUTH_CACHE_SIZE: Limit = Limit::positive("IRONFLOW_OAUTH_CACHE_SIZE", 128);
/// Entries kept in the process-global `cache_set` / `cache_get` memory backend.
pub(crate) const CACHE_MAX_ENTRIES: Limit = Limit::positive("IRONFLOW_CACHE_MAX_ENTRIES", 10_000);
/// Live MCP session handles; least-recently used sessions are closed first.
pub(crate) const MCP_SESSION_CACHE_SIZE: Limit =
    Limit::positive("IRONFLOW_MCP_SESSION_CACHE_SIZE", 1_024);
/// Idle TTL for live MCP sessions, in seconds.
pub(crate) const MCP_SESSION_TTL_SECS: Limit =
    Limit::positive("IRONFLOW_MCP_SESSION_TTL_SECS", 3_600);

fn capacity(limit: &Limit) -> usize {
    usize::try_from(limit.read()).unwrap_or(usize::MAX)
}

pub fn max_detached_subworkflows() -> usize {
    capacity(&MAX_DETACHED_SUBWORKFLOWS)
}

pub fn oauth_cache_size() -> usize {
    capacity(&OAUTH_CACHE_SIZE)
}

pub fn cache_max_entries() -> usize {
    capacity(&CACHE_MAX_ENTRIES)
}

pub fn mcp_session_cache_size() -> usize {
    capacity(&MCP_SESSION_CACHE_SIZE)
}

pub fn mcp_session_ttl_secs() -> u64 {
    MCP_SESSION_TTL_SECS.read()
}
