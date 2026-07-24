use std::sync::LazyLock;

use super::RedisStateStore;
use super::listing::run_id_from_member;
use crate::storage::StorageResult;
use crate::storage::redis_config::map_redis_error;

/// Catalog entries inspected independently of the user-visible page cursor.
///
/// Keeping this fixed bounds the incremental maintenance added to each
/// steady-state page while the persistent Redis cursor guarantees that
/// repeatedly reading only the newest page still traverses the complete
/// catalog over time. Initial legacy-catalog rebuild remains a separate scan.
const MAINTENANCE_BATCH_SIZE: usize = 32;

const MAINTENANCE_BATCH_SCRIPT_SOURCE: &str = include_str!("scripts/maintenance_batch.lua");

static MAINTENANCE_BATCH_SCRIPT: LazyLock<redis::Script> =
    LazyLock::new(|| redis::Script::new(MAINTENANCE_BATCH_SCRIPT_SOURCE));

impl RedisStateStore {
    async fn claim_catalog_maintenance_batch(&self) -> StorageResult<Vec<String>> {
        let mut conn = self.conn.clone();
        MAINTENANCE_BATCH_SCRIPT
            .key(self.ordered_catalog_key())
            .key(self.ordered_catalog_maintenance_cursor_key())
            .key(self.ordered_catalog_maintenance_high_water_key())
            .arg(MAINTENANCE_BATCH_SIZE)
            .invoke_async(&mut conn)
            .await
            .map_err(|error| {
                map_redis_error("Failed to advance Redis run-catalog maintenance", error)
            })
    }

    pub(super) async fn maintain_ordered_catalog(&self) -> StorageResult<()> {
        for member in self.claim_catalog_maintenance_batch().await? {
            let run_id = run_id_from_member(&member)?;
            self.maintain_ordered_catalog_entry(run_id).await?;
        }
        Ok(())
    }
}
