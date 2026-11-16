//! Deletion of one run from the Redis state store.

use super::RedisStateStore;
use super::atomic::DELETE_SCRIPT;
use crate::storage::redis_config::map_redis_error;
use crate::storage::{DeletedRun, StorageError, StorageResult};

impl RedisStateStore {
    /// Delete one run unless a live owner is still executing it. The script
    /// returns the incarnation recorded with the run it removed (IF-214).
    pub(super) async fn delete_run_atomic(&self, run_id: &str) -> StorageResult<DeletedRun> {
        let run_key = self.resolve_run_key(run_id).await?;
        let mut conn = self.conn.clone();
        let status_keys = self.ordered_status_keys();
        let (deleted, incarnation): (i64, String) = DELETE_SCRIPT
            .key(run_key)
            .key(self.index_key())
            .key(self.ordered_catalog_members_key())
            .key(self.ordered_catalog_key())
            .key(&status_keys[0])
            .key(&status_keys[1])
            .key(&status_keys[2])
            .key(&status_keys[3])
            .key(&status_keys[4])
            .key(&status_keys[5])
            .key(self.run_lease_expiry_key())
            .arg(run_id)
            .invoke_async(&mut conn)
            .await
            .map_err(|error| {
                map_redis_error(format_args!("Failed to delete Redis run '{run_id}'"), error)
            })?;
        match deleted {
            1 => Ok(DeletedRun {
                incarnation: Some(incarnation).filter(|incarnation| !incarnation.is_empty()),
            }),
            2 => Err(StorageError::conflict(format_args!(
                "Run '{run_id}' is still executing"
            ))),
            0 => Err(StorageError::not_found(format_args!(
                "Run '{run_id}' not found"
            ))),
            _ => Err(StorageError::corruption(
                format_args!("Invalid deletion result for Redis run '{run_id}'"),
                deleted,
            )),
        }
    }
}
