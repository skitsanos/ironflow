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
    // A schedule naming a flow outside `flows_dir` — a typo, a moved file —
    // must fail the process now, not surface as a `WARN` at its first due
    // instant.
    crate::scheduler::startup::validate_schedule_flows(&schedules, options.flows_dir.as_deref())?;

    // Binding and API policy construction are a hard barrier before any
    // background work starts. A bad address, auth policy, or CORS origin must
    // never allow an immediately-due schedule to fire first.
    let scheduler_flows_dir = options.flows_dir.clone();
    let scheduler_max_concurrent_tasks = options.max_concurrent_tasks;
    let server = crate::api::prepare(store.clone(), event_store.clone(), options)
        .await?
        .start_run_lifecycle(store.clone())
        .await?;

    // Spawned here rather than inside `api::serve` so schedules stay off
    // `ServeOptions` and the REST surface keeps one responsibility.
    let scheduler = crate::scheduler::spawn(
        schedules,
        store.clone(),
        event_store.clone(),
        scheduler_flows_dir,
        scheduler_max_concurrent_tasks,
    );

    match scheduler {
        Some(scheduler) => scheduler.supervise(server.serve()).await,
        None => server.serve().await,
    }
}
