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

    /// Value of a claim key whose occurrence is settled: it has a durable run
    /// (IF-201) or the scheduler skipped it (IF-217).
    ///
    /// A plain claim stores `key` itself, so prefixing `key` can never equal
    /// the value of an unmarked claim for the same instant.
    fn run_recorded_value(key: &str) -> String {
        format!("run:{key}")
    }

    /// Rewrite the claim key's value in place. Unconditional, so the record
    /// is written even if the claim key is missing, and `EX` restarts the
    /// claim's lifetime from the moment the run exists.
    pub(super) async fn record_schedule_run_key(
        &self,
        name: &str,
        key: &str,
        ttl_seconds: u64,
    ) -> StorageResult<()> {
        let mut conn = self.conn.clone();
        let _: () = conn
            .set_ex(
                self.schedule_claim_key(name, key),
                Self::run_recorded_value(key),
                ttl_seconds,
            )
            .await
            .map_err(|error| StorageError::backend("Record scheduled run", error))?;
        Ok(())
    }

    pub(super) async fn schedule_run_recorded_key(
        &self,
        name: &str,
        key: &str,
    ) -> StorageResult<bool> {
        let mut conn = self.conn.clone();
        let value: Option<String> = conn
            .get(self.schedule_claim_key(name, key))
            .await
            .map_err(|error| StorageError::backend("Read scheduled run record", error))?;
        Ok(value.is_some_and(|value| value == Self::run_recorded_value(key)))
    }
}
