//! Executing a claimed schedule through the workflow engine.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;

use crate::api::handlers::resolve_flow_path_in;
use crate::engine::WorkflowEngine;
use crate::lua::LuaRuntime;
use crate::nodes::NodeRegistry;
use crate::storage::StateStore;
use crate::storage::event_store::EventStore;
use crate::util::sensitive_url::redact_sensitive_text;

use super::config::ScheduleConfig;
use super::{ScheduleExecutor, ScheduleRun};

pub use super::context::SCHEDULE_CONTEXT_KEY;
pub use super::overlap::MAX_OVERLAP_SCAN;

/// Longest a scheduled start waits for a flow-load permit before it is
/// deferred to the next tick. Well inside `SCHEDULE_EVALUATION_TIMEOUT`, so
/// a full limit defers the instant instead of timing the evaluation out.
pub const FLOW_LOAD_WAIT: Duration = Duration::from_secs(5);

const FLOW_LOAD_DEFERRED: &str = "server is at maximum concurrent flow-loading capacity";

/// Runs schedules through the same engine path the REST API uses.
pub struct FlowExecutor {
    registry: Arc<NodeRegistry>,
    store: Arc<dyn StateStore>,
    event_store: Arc<dyn EventStore>,
    flows_dir: Option<PathBuf>,
    max_concurrent_tasks: Option<usize>,
    lifecycle: crate::api::ServiceLifecycle,
    metrics: Option<Arc<crate::metrics::Metrics>>,
    flow_load_wait: Duration,
}

impl FlowExecutor {
    pub fn new(
        registry: Arc<NodeRegistry>,
        store: Arc<dyn StateStore>,
        event_store: Arc<dyn EventStore>,
        flows_dir: Option<PathBuf>,
        max_concurrent_tasks: Option<usize>,
    ) -> Self {
        Self::new_with_lifecycle(
            registry,
            store,
            event_store,
            flows_dir,
            max_concurrent_tasks,
            crate::api::ServiceLifecycle::default(),
            None,
        )
    }

    pub(crate) fn new_with_lifecycle(
        registry: Arc<NodeRegistry>,
        store: Arc<dyn StateStore>,
        event_store: Arc<dyn EventStore>,
        flows_dir: Option<PathBuf>,
        max_concurrent_tasks: Option<usize>,
        lifecycle: crate::api::ServiceLifecycle,
        metrics: Option<Arc<crate::metrics::Metrics>>,
    ) -> Self {
        Self {
            registry,
            store,
            event_store,
            flows_dir,
            max_concurrent_tasks,
            lifecycle,
            metrics,
            flow_load_wait: FLOW_LOAD_WAIT,
        }
    }

    /// Override how long a start waits for a flow-load permit, for embedded
    /// runtimes and deterministic admission tests.
    pub fn with_flow_load_wait(mut self, wait: Duration) -> Self {
        self.flow_load_wait = wait;
        self
    }

    /// Embedded one-shot execution helper retained independently of cron
    /// occurrence identity. The scheduler itself calls the trait method with
    /// its durable instant key.
    pub async fn run(
        &self,
        schedule_name: &str,
        schedule: &ScheduleConfig,
    ) -> Result<String, String> {
        let instant_key = format!("embedded@{}", uuid::Uuid::new_v4().simple());
        match <Self as ScheduleExecutor>::run(self, schedule_name, &instant_key, schedule).await? {
            ScheduleRun::Started { run_id } | ScheduleRun::Existing { run_id } => Ok(run_id),
            ScheduleRun::Deferred { reason } => Err(reason),
        }
    }
}

#[async_trait]
impl ScheduleExecutor for FlowExecutor {
    async fn active_run(&self, schedule_name: &str) -> Option<String> {
        super::overlap::active_run(self.store.as_ref(), schedule_name).await
    }

    fn has_capacity(&self) -> bool {
        // The permit is dropped immediately; this is a probe, not a
        // reservation. `run` acquires the permit it actually holds.
        crate::api::acquire_run_permit(&self.lifecycle, None).is_ok()
    }

    async fn run(
        &self,
        schedule_name: &str,
        instant_key: &str,
        schedule: &ScheduleConfig,
    ) -> Result<ScheduleRun, String> {
        let path = resolve_flow_path_in(schedule.flow(), self.flows_dir.as_deref())
            .map_err(|error| format!("{error:?}"))?;

        // Held for the run's real duration: moved into the spawned task below,
        // not dropped when this function returns. Reserve it before the Lua VM
        // is created so a run rejected for capacity cannot still consume parse
        // CPU and memory.
        let run_permit = crate::api::acquire_run_permit(&self.lifecycle, self.metrics.as_deref())
            .map_err(|_| "server is at maximum concurrent run capacity".to_string())?;
        // Schedules sharing a tick queue for the load limit instead of failing
        // fast like API requests. A wait that expires has parsed nothing and
        // created no run, so the occurrence is deferred, not consumed (IF-182).
        let Some(flow_load_permit) = crate::api::acquire_flow_load_permit_within(
            self.flow_load_wait,
            self.metrics.as_deref(),
        )
        .await
        .map_err(|error| format!("{error:?}"))?
        else {
            return Ok(ScheduleRun::Deferred {
                reason: FLOW_LOAD_DEFERRED.to_string(),
            });
        };

        // Parse off the async runtime so a pathological flow cannot pin a
        // worker thread and stall the server (IF-038). The error can echo
        // file-derived tokens (`near '<token>'`), so it is redacted exactly as
        // the REST API redacts the same failure (see `helpers::flow_file_load_error`)
        // before it becomes a string that gets logged.
        let registry = self.registry.clone();
        let load_path = path.clone();
        let flow = crate::api::supervise_flow_load(
            flow_load_permit,
            async move { LuaRuntime::load_flow_async(&load_path, &registry).await },
            self.metrics.clone(),
        )
        .await
        .map_err(|error| {
            format!(
                "failed to load flow: {}",
                redact_sensitive_text(&format!("{error:#}"))
            )
        })?;

        let initial_ctx =
            super::context::initial_context(schedule_name, instant_key, schedule, &path);

        let engine = WorkflowEngine::new_with_events(
            self.registry.clone(),
            self.store.clone(),
            self.event_store.clone(),
            self.max_concurrent_tasks,
        )
        .with_metrics(self.metrics.clone());

        // Start and return once the run exists, without awaiting it to
        // completion. The tick loop awaits `evaluate`, which awaits `decide`,
        // which awaits this: if this awaited the whole run, one long-running
        // flow would starve every other schedule's next tick, and a flow with
        // no step deadline that never returns would silently end all
        // scheduling for the process's lifetime. `RunHandle::wait` retains
        // detach semantics, which is exactly what running it in a background
        // task needs.
        let run_id = super::identity::run_id(schedule_name, instant_key);
        let Some(handle) = engine
            .start_with_run_id(&flow, initial_ctx, run_id.clone())
            .await
            .map_err(|error| format!("{error:#}"))?
        else {
            return Ok(ScheduleRun::Existing { run_id });
        };
        let waited_run_id = run_id.clone();
        let schedule_name = schedule_name.to_string();

        let lifecycle = self.lifecycle.clone();
        tokio::spawn(async move {
            // Held until the run finishes, exactly as the API does for a
            // synchronous run (IF-042); the run outliving `run()` is the point
            // of this change, so the permit must too.
            if let Err(error) =
                crate::api::wait_for_admitted_run(lifecycle, handle, run_permit).await
            {
                // Nothing else observes this run again: `decide` already
                // logged `Fired` with this run id, and no later tick will
                // revisit it.
                tracing::warn!(
                    schedule = %schedule_name,
                    run_id = %waited_run_id,
                    %error,
                    "scheduled run failed after starting"
                );
            }
        });

        Ok(ScheduleRun::Started { run_id })
    }
}
