use std::sync::LazyLock;

use uuid::Uuid;

use super::RedisStateStore;
use crate::engine::types::{RunInfo, RunSummary};
use crate::storage::redis_config::map_redis_error;
use crate::storage::{RunLease, StorageError, StorageResult};

const INIT_SOURCE: &str = include_str!("scripts/init.lua");
static INIT_SCRIPT: LazyLock<redis::Script> = LazyLock::new(|| redis::Script::new(INIT_SOURCE));

impl RedisStateStore {
    pub(super) async fn initialize_run(
        &self,
        info: &RunInfo,
        lease: Option<&RunLease>,
    ) -> StorageResult<()> {
        let run_key = self.resolve_run_key(&info.id).await?;
        let mut conn = self.conn.clone();
        let raw_info = serialize(info, "run")?;
        let summary = RunSummary::from(info);
        let raw_summary = serialize(&summary, "run summary")?;
        let revision = Uuid::new_v4().simple().to_string();
        let incarnation = Uuid::new_v4().simple().to_string();
        let ordered_member = super::listing::ordered_member(&summary);
        let status_keys = self.ordered_status_keys();

        let initialized: i64 = INIT_SCRIPT
            .key(&run_key)
            .key(self.index_key())
            .key(self.ordered_catalog_members_key())
            .key(self.ordered_catalog_key())
            .key(&status_keys[0])
            .key(&status_keys[1])
            .key(&status_keys[2])
            .key(&status_keys[3])
            .key(&status_keys[4])
            .key(&status_keys[5])
            .key(self.ordered_catalog_ready_key())
            .key(self.run_lease_expiry_key())
            .arg(&raw_info)
            .arg(&raw_summary)
            .arg(&revision)
            .arg(&incarnation)
            .arg(&info.id)
            .arg(self.ttl.unwrap_or(-1))
            .arg(&ordered_member)
            .arg(info.status.to_string())
            .arg(lease.map(RunLease::owner).unwrap_or(""))
            .arg(crate::storage::RUN_LEASE_TTL.as_micros())
            .arg(crate::storage::run_lease::RUN_LEASE_KEY_SAFETY.as_micros())
            .invoke_async(&mut conn)
            .await
            .map_err(|error| {
                map_redis_error(
                    format_args!("Failed to initialize Redis run '{}'", info.id),
                    error,
                )
            })?;
        match initialized {
            1 => Ok(()),
            0 => Err(StorageError::conflict(format_args!(
                "Run '{}' already exists",
                info.id
            ))),
            value => Err(StorageError::corruption(
                format_args!("Invalid initialization result for Redis run '{}'", info.id),
                value,
            )),
        }
    }
}

fn serialize<T: serde::Serialize>(value: &T, kind: &str) -> StorageResult<String> {
    serde_json::to_string(value).map_err(|error| {
        StorageError::backend(format_args!("Failed to serialize Redis {kind}"), error)
    })
}
