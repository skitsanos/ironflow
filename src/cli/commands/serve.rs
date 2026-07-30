use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;

use crate::scheduler::config::ScheduleConfig;
use crate::storage::StateStore;
use crate::storage::event_store::EventStore;

/// Execute the `serve` subcommand.
pub(crate) async fn cmd_serve(
    store: Arc<dyn StateStore>,
    event_store: Arc<dyn EventStore>,
    options: crate::api::ServeOptions,
    schedules: HashMap<String, ScheduleConfig>,
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

    // A schedule naming a flow outside `flows_dir` — a typo, a moved file —
    // must fail the process now, not surface as a `WARN` at its first due
    // instant.
    crate::scheduler::startup::validate_schedule_flows(&schedules, options.flows_dir.as_deref())?;

    // Spawned here rather than inside `api::serve` so schedules stay off
    // `ServeOptions` and the REST surface keeps one responsibility.
    let _scheduler = crate::scheduler::spawn(
        schedules,
        store.clone(),
        event_store.clone(),
        options.flows_dir.clone(),
        options.max_concurrent_tasks,
    );

    crate::api::serve(store, event_store, options).await
}
