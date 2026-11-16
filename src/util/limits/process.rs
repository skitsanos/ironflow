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
/// Seconds blocking workers may keep the process alive after a command has
/// returned (IF-183, IF-220). The ceiling keeps the timer arithmetic in range.
pub(crate) const RUNTIME_SHUTDOWN_SECONDS: Limit =
    Limit::bounded("IRONFLOW_RUNTIME_SHUTDOWN_SECONDS", 5, 3_600);
/// Largest `max_iterations` a `repeat_subworkflow` step may request. The
/// ceiling of 1024 is fixed (IF-220: validated at startup like the others).
pub(crate) const MAX_REPEAT_ITERATIONS: Limit =
    Limit::bounded("IRONFLOW_MAX_REPEAT_ITERATIONS", 128, 1_024);

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

pub fn max_repeat_iterations() -> usize {
    capacity(&MAX_REPEAT_ITERATIONS)
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

/// How long detached blocking workers may hold the process once the command
/// has returned; a worker inside an uninterruptible call is then abandoned.
pub fn runtime_shutdown_timeout() -> std::time::Duration {
    std::time::Duration::from_secs(RUNTIME_SHUTDOWN_SECONDS.read())
}
