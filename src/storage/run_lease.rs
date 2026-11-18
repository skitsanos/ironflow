use jiff::Timestamp;
use uuid::Uuid;

/// How long a workflow coordinator owns its durable run without a heartbeat.
///
/// A lease is deliberately much longer than its refresh interval so one slow
/// runtime turn does not let a restarting peer terminalize healthy work.
pub const RUN_LEASE_TTL: std::time::Duration = std::time::Duration::from_secs(90);

/// How often an active coordinator extends its ownership lease.
pub const RUN_LEASE_REFRESH: std::time::Duration = std::time::Duration::from_secs(30);

/// Extra Redis key lifetime beyond the lease deadline. Three complete reaper
/// intervals cover one delayed tick, one timed-out pass, and scheduling jitter.
#[cfg(feature = "redis")]
pub(crate) const RUN_LEASE_KEY_SAFETY: std::time::Duration = std::time::Duration::from_secs(90);

/// Fenced ownership metadata attached to one workflow run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunLease {
    owner: String,
    expires_at: Timestamp,
}

impl RunLease {
    /// Create a fresh process-unique ownership token.
    pub fn fresh() -> Self {
        Self::renewed(Uuid::new_v4().simple().to_string())
    }

    /// Extend `owner` from the current wall clock by the standard TTL.
    pub fn renewed(owner: String) -> Self {
        Self {
            owner,
            // A duration cannot fail to add; only a calendar span can.
            expires_at: Timestamp::now()
                .saturating_add(RUN_LEASE_TTL)
                .unwrap_or(Timestamp::MAX),
        }
    }

    /// Build deterministic lease metadata for storage-contract tests.
    pub fn at(owner: impl Into<String>, expires_at: Timestamp) -> Self {
        Self {
            owner: owner.into(),
            expires_at,
        }
    }

    pub fn owner(&self) -> &str {
        &self.owner
    }

    pub fn expires_at(&self) -> Timestamp {
        self.expires_at
    }

    pub fn expires_micros(&self) -> i64 {
        crate::util::timestamp::unix_micros(self.expires_at)
    }
}
