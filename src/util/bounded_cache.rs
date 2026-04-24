//! Bounded in-memory cache with LRU eviction and optional TTL.
//!
//! Used by nodes that keep process-global state (currently `cache_node` and the
//! MCP session tracker) to guarantee a hard upper bound on memory use.
//!
//! Design notes:
//! - Eviction is O(n) on insert when at capacity. That's fine for our target
//!   sizes (thousands of entries). Swap to a proper linked-list LRU only if
//!   benchmarks show a real hotspot.
//! - Expiry is proactive: every insert runs a sweep of expired entries before
//!   deciding whether eviction is needed. `get` also removes the hit entry if
//!   it has expired.

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// Per-entry metadata stored alongside the value.
struct Entry<V> {
    value: V,
    expires_at_secs: Option<u64>,
    last_access: u64,
}

impl<V> Entry<V> {
    fn is_expired(&self, now_secs: u64) -> bool {
        matches!(self.expires_at_secs, Some(exp) if now_secs >= exp)
    }
}

struct Inner<K, V> {
    map: HashMap<K, Entry<V>>,
    tick: u64,
}

/// Thread-safe bounded cache with LRU eviction and per-entry TTL.
pub struct BoundedCache<K, V> {
    inner: Mutex<Inner<K, V>>,
    max_entries: usize,
}

impl<K, V> BoundedCache<K, V>
where
    K: Eq + Hash + Clone,
    V: Clone,
{
    pub fn new(max_entries: usize) -> Self {
        assert!(max_entries > 0, "BoundedCache max_entries must be > 0");
        Self {
            inner: Mutex::new(Inner {
                map: HashMap::new(),
                tick: 0,
            }),
            max_entries,
        }
    }

    pub fn len(&self) -> usize {
        self.inner.lock().map(|g| g.map.len()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn max_entries(&self) -> usize {
        self.max_entries
    }

    /// Insert a value. Drops expired entries first, then evicts the
    /// least-recently-used entry if still at capacity.
    pub fn insert(&self, key: K, value: V, ttl_secs: Option<u64>) {
        let Ok(mut inner) = self.inner.lock() else {
            return;
        };
        let now = now_secs();

        inner.map.retain(|_, e| !e.is_expired(now));

        if !inner.map.contains_key(&key)
            && inner.map.len() >= self.max_entries
            && let Some(victim) = inner
                .map
                .iter()
                .min_by_key(|(_, e)| e.last_access)
                .map(|(k, _)| k.clone())
        {
            inner.map.remove(&victim);
        }

        inner.tick = inner.tick.wrapping_add(1);
        let tick = inner.tick;
        let expires_at_secs = ttl_secs.map(|t| now.saturating_add(t));
        inner.map.insert(
            key,
            Entry {
                value,
                expires_at_secs,
                last_access: tick,
            },
        );
    }

    /// Fetch a value. Returns `None` and removes the entry if it has expired.
    pub fn get(&self, key: &K) -> Option<V> {
        let Ok(mut inner) = self.inner.lock() else {
            return None;
        };
        let now = now_secs();

        let expired = matches!(inner.map.get(key), Some(e) if e.is_expired(now));
        if expired {
            inner.map.remove(key);
            return None;
        }

        inner.tick = inner.tick.wrapping_add(1);
        let tick = inner.tick;
        let entry = inner.map.get_mut(key)?;
        entry.last_access = tick;
        Some(entry.value.clone())
    }

    pub fn contains_key(&self, key: &K) -> bool {
        self.get(key).is_some()
    }

    pub fn remove(&self, key: &K) -> Option<V> {
        let mut inner = self.inner.lock().ok()?;
        inner.map.remove(key).map(|e| e.value)
    }

    pub fn clear(&self) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.map.clear();
        }
    }

    /// Remove all expired entries. Returns the number removed.
    pub fn sweep_expired(&self) -> usize {
        let Ok(mut inner) = self.inner.lock() else {
            return 0;
        };
        let before = inner.map.len();
        let now = now_secs();
        inner.map.retain(|_, e| !e.is_expired(now));
        before - inner.map.len()
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_and_get() {
        let c: BoundedCache<String, i32> = BoundedCache::new(4);
        c.insert("a".into(), 1, None);
        c.insert("b".into(), 2, None);
        assert_eq!(c.get(&"a".into()), Some(1));
        assert_eq!(c.get(&"b".into()), Some(2));
        assert_eq!(c.len(), 2);
    }

    #[test]
    fn lru_eviction_on_capacity() {
        let c: BoundedCache<String, i32> = BoundedCache::new(3);
        c.insert("a".into(), 1, None);
        c.insert("b".into(), 2, None);
        c.insert("c".into(), 3, None);
        // Touch a and b so c is the LRU
        let _ = c.get(&"a".into());
        let _ = c.get(&"b".into());
        c.insert("d".into(), 4, None);
        assert_eq!(c.len(), 3);
        assert_eq!(c.get(&"c".into()), None, "c should be evicted (LRU)");
        assert_eq!(c.get(&"a".into()), Some(1));
        assert_eq!(c.get(&"b".into()), Some(2));
        assert_eq!(c.get(&"d".into()), Some(4));
    }

    #[test]
    fn overwriting_existing_key_does_not_evict() {
        let c: BoundedCache<String, i32> = BoundedCache::new(2);
        c.insert("a".into(), 1, None);
        c.insert("b".into(), 2, None);
        c.insert("a".into(), 10, None);
        assert_eq!(c.len(), 2);
        assert_eq!(c.get(&"a".into()), Some(10));
        assert_eq!(c.get(&"b".into()), Some(2));
    }

    #[test]
    fn ttl_expiry_on_get() {
        let c: BoundedCache<String, i32> = BoundedCache::new(4);
        c.insert("a".into(), 1, Some(0)); // expires immediately (now >= now+0)
        std::thread::sleep(std::time::Duration::from_millis(1100));
        assert_eq!(c.get(&"a".into()), None);
        assert_eq!(c.len(), 0, "expired entry should be removed on get");
    }

    #[test]
    fn sweep_expired_reclaims_without_read() {
        let c: BoundedCache<String, i32> = BoundedCache::new(10);
        // ttl=1 so entries survive the implicit sweep inside `c` but expire
        // before the explicit `sweep_expired()` call below.
        c.insert("a".into(), 1, Some(1));
        c.insert("b".into(), 2, Some(1));
        c.insert("c".into(), 3, None);
        std::thread::sleep(std::time::Duration::from_millis(1500));
        let removed = c.sweep_expired();
        assert_eq!(removed, 2);
        assert_eq!(c.len(), 1);
        assert_eq!(c.get(&"c".into()), Some(3));
    }

    #[test]
    fn hard_bound_under_pressure() {
        let c: BoundedCache<String, i32> = BoundedCache::new(50);
        for i in 0..500 {
            c.insert(format!("k{i}"), i, None);
        }
        assert_eq!(
            c.len(),
            50,
            "cache must never exceed max_entries even under heavy insert pressure"
        );
    }

    #[test]
    fn remove_returns_value() {
        let c: BoundedCache<String, i32> = BoundedCache::new(4);
        c.insert("a".into(), 1, None);
        assert_eq!(c.remove(&"a".into()), Some(1));
        assert_eq!(c.remove(&"a".into()), None);
    }
}
