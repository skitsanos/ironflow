use std::sync::Arc;

use anyhow::Result;

use crate::storage::StateStore;
use crate::storage::event_store::EventStore;

/// Execute the `serve` subcommand.
pub(crate) async fn cmd_serve(
    store: Arc<dyn StateStore>,
    event_store: Arc<dyn EventStore>,
    options: crate::api::ServeOptions,
) -> Result<()> {
    // Reconcile runs left non-terminal by a previous process before accepting
    // traffic, so a crash/restart cannot strand runs as `Running` forever
    // (IF-043).
    match crate::storage::reconcile_nonterminal_runs(store.as_ref()).await {
        Ok(0) => {}
        Ok(count) => tracing::info!(
            count,
            "reconciled non-terminal runs as Stalled after restart"
        ),
        Err(error) => tracing::warn!(%error, "startup run reconciliation failed; continuing"),
    }

    crate::api::serve(store, event_store, options).await
}
