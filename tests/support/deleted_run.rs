//! IF-214: a state delete reports the incarnation of the run it removed.

use ironflow::engine::types::{Context, RunStatus};
use ironflow::storage::{RunLease, StateStore, StorageErrorKind};

/// `recorded` is false for a store that keeps the default and reports none.
pub async fn assert_delete_reports_the_removed_incarnation(
    store: &dyn StateStore,
    prefix: &str,
    recorded: bool,
) {
    let run_id = format!("{prefix}-owned");
    for owner in ["incarnation-a", "incarnation-b"] {
        store
            .init_run_owned(
                &run_id,
                "flow",
                &Context::new(),
                &RunLease::renewed(owner.to_string()),
            )
            .await
            .unwrap();
        // A live owner keeps the run, and nothing is reported as removed.
        assert_eq!(
            store
                .delete_run_reporting(&run_id)
                .await
                .unwrap_err()
                .kind(),
            StorageErrorKind::Conflict
        );
        // The terminal transition releases the lease; the incarnation stays.
        assert!(
            store
                .set_run_status_owned(&run_id, RunStatus::Success, owner)
                .await
                .unwrap()
        );
        let deleted = store.delete_run_reporting(&run_id).await.unwrap();
        assert_eq!(deleted.incarnation.as_deref(), recorded.then_some(owner));
        assert_eq!(
            store
                .delete_run_reporting(&run_id)
                .await
                .unwrap_err()
                .kind(),
            StorageErrorKind::NotFound
        );
    }

    // A run created without a lease has no incarnation to report.
    let unowned = format!("{prefix}-unowned");
    store
        .init_run(&unowned, "flow", &Context::new())
        .await
        .unwrap();
    let deleted = store.delete_run_reporting(&unowned).await.unwrap();
    assert_eq!(deleted.incarnation, None);
}
