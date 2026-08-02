//! Cross-replica schedule claims backed by `SET NX EX`.

use redis::AsyncCommands;

use super::RedisStateStore;
use crate::storage::{StorageError, StorageResult};

impl RedisStateStore {
    /// Key for one claim: `{prefix}schedule_claims:{hex(name)}:{hex(key)}`.
    ///
    /// `name` and `key` are hex-encoded into separate segments rather than
    /// joined and encoded together. Hex output can never contain the `:`
    /// separator, so the mapping is injective unconditionally — it does not
    /// depend on any character being absent from an operator-supplied schedule
    /// name. No schedule can alias another's claim or IronFlow's own
    /// bookkeeping keys, the same reason run ids are escaped.
    fn schedule_claim_key(&self, name: &str, key: &str) -> String {
        format!(
            "{}schedule_claims:{}:{}",
            self.prefix,
            hex::encode(name.as_bytes()),
            hex::encode(key.as_bytes())
        )
    }

    pub(super) async fn claim_schedule_key(
        &self,
        name: &str,
        key: &str,
        ttl_seconds: u64,
    ) -> StorageResult<bool> {
        let mut conn = self.conn.clone();
        // `NX` makes the write the coordination primitive and `EX` retires the
        // claim without any sweep of our own.
        let claimed: bool = conn
            .set_options(
                self.schedule_claim_key(name, key),
                key,
                redis::SetOptions::default()
                    .conditional_set(redis::ExistenceCheck::NX)
                    .with_expiration(redis::SetExpiry::EX(ttl_seconds)),
            )
            .await
            .map_err(|error| StorageError::backend("Claim scheduled instant", error))?;
        Ok(claimed)
    }
}
