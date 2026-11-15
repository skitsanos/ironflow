use super::event_store::EventStore;
use super::{StateStore, StorageResult};

/// What [`StateStore::delete_run_reporting`] removed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeletedRun {
    /// The run incarnation the deleted record was created for: the owner of
    /// the lease passed to [`StateStore::init_run_owned`]. `None` when the
    /// store does not record it or the run predates the record.
    pub incarnation: Option<String>,
}

/// Delete one run across independently configured state and event stores.
///
/// State is removed first. Event deletion is idempotent and installs a
/// publication fence against the deleted incarnation, so retrying after an
/// interrupted event cleanup removes any orphaned stream without allowing a
/// late workflow event to recreate it. A missing state record is reported
/// only when no orphaned events existed, and in that case nothing is fenced:
/// the ID stays usable by a run that is created later (IF-184).
///
/// The two steps are not one transaction, so a run with the same
/// deterministic ID can be accepted between them. The event step therefore
/// names the incarnation whose state was removed and leaves the stream alone
/// once it has admitted another one (IF-214). The state store reports that
/// incarnation when it records it; otherwise the admission read before the
/// state delete stands in for it, and that read also guards the sweep of
/// orphaned events.
pub async fn delete_run(
    state: &dyn StateStore,
    events: &dyn EventStore,
    run_id: &str,
) -> StorageResult<()> {
    let seen = events.admitted_incarnation(run_id).await?;
    match state.delete_run_reporting(run_id).await {
        Ok(deleted) => {
            match deleted.incarnation.as_deref().or(seen.as_deref()) {
                Some(incarnation) => events.delete_incarnation(run_id, incarnation).await?,
                None => events.delete_run(run_id).await?,
            };
            Ok(())
        }
        Err(error) if error.is_not_found() => {
            let removed = events
                .delete_orphaned_incarnation(run_id, seen.as_deref())
                .await?;
            if removed == 0 { Err(error) } else { Ok(()) }
        }
        Err(error) => Err(error),
    }
}
