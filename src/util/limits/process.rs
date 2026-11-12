//! Process-wide capacities. Most are read once into a `OnceLock` or
//! `LazyLock`, so startup validation is what makes a typo visible: by the time
//! a node reads one, the value is already baked into the process. The child
//! workflow ceilings are read on each use.

use super::Limit;

/// Detached `subworkflow` executions admitted at once.
pub(crate) const MAX_DETACHED_SUBWORKFLOWS: Limit =
    Limit::positive("IRONFLOW_MAX_DETACHED_SUBWORKFLOWS", 64);
/// Child workflow levels below a top-level run (IF-188). A waiting, repeated,
/// parallel, tool or detached child at a deeper level is refused.
pub(crate) const MAX_SUBWORKFLOW_DEPTH: Limit =
    Limit::positive("IRONFLOW_MAX_SUBWORKFLOW_DEPTH", 16);
/// Child flows one `parallel_subworkflows` call may fan out to (IF-188).
pub(crate) const MAX_PARALLEL_SUBWORKFLOWS: Limit =
    Limit::positive("IRONFLOW_MAX_PARALLEL_SUBWORKFLOWS", 1_024);
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

pub fn max_subworkflow_depth() -> usize {
    capacity(&MAX_SUBWORKFLOW_DEPTH)
}

pub fn max_parallel_subworkflows() -> usize {
    capacity(&MAX_PARALLEL_SUBWORKFLOWS)
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
