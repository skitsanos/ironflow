use super::event_store::EventStore;
use super::{StateStore, StorageResult};

/// Delete one run across independently configured state and event stores.
///
/// State is removed first. Event deletion is idempotent and installs a
/// publication fence, so retrying after an interrupted event cleanup removes
/// any orphaned stream without allowing a late workflow event to recreate it.
/// A missing state record is reported only when no orphaned events existed.
pub async fn delete_run(
    state: &dyn StateStore,
    events: &dyn EventStore,
    run_id: &str,
) -> StorageResult<()> {
    match state.delete_run(run_id).await {
        Ok(()) => {
            events.delete_run(run_id).await?;
            Ok(())
        }
        Err(error) if error.is_not_found() => {
            if events.delete_run(run_id).await? == 0 {
                Err(error)
            } else {
                Ok(())
            }
        }
        Err(error) => Err(error),
    }
}
