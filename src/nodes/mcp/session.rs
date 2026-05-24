use std::sync::OnceLock;

use crate::util::bounded_cache::BoundedCache;

/// Maximum distinct `(url, session_id)` pairs remembered as "already initialized".
/// Override with `IRONFLOW_MCP_SESSION_CACHE_SIZE`.
const DEFAULT_SESSION_CACHE_SIZE: usize = 1024;
/// Idle TTL for a remembered MCP session. Override with
/// `IRONFLOW_MCP_SESSION_TTL_SECS`.
const DEFAULT_SESSION_TTL_SECS: u64 = 3600;

static INITIALIZED_SESSIONS: OnceLock<BoundedCache<String, ()>> = OnceLock::new();

fn session_cache_capacity() -> usize {
    std::env::var("IRONFLOW_MCP_SESSION_CACHE_SIZE")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|n| *n > 0)
        .unwrap_or(DEFAULT_SESSION_CACHE_SIZE)
}

fn session_cache_ttl() -> u64 {
    std::env::var("IRONFLOW_MCP_SESSION_TTL_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|n| *n > 0)
        .unwrap_or(DEFAULT_SESSION_TTL_SECS)
}

fn initialized_sessions() -> &'static BoundedCache<String, ()> {
    INITIALIZED_SESSIONS.get_or_init(|| BoundedCache::new(session_cache_capacity()))
}

pub(super) fn session_cache_key(url: &str, session_id: &str) -> String {
    format!("{url}::{session_id}")
}

pub(super) fn is_session_initialized(url: &str, session_id: &str) -> bool {
    let key = session_cache_key(url, session_id);
    initialized_sessions().contains_key(&key)
}

pub(super) fn mark_session_initialized(url: &str, session_id: &str) {
    let key = session_cache_key(url, session_id);
    initialized_sessions().insert(key, (), Some(session_cache_ttl()));
}
