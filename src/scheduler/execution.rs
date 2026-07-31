//! Executing a claimed schedule through the workflow engine.

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;

use crate::api::handlers::resolve_flow_path_in;
use crate::engine::WorkflowEngine;
use crate::engine::types::{Context, RunStatus};
use crate::lua::LuaRuntime;
use crate::nodes::NodeRegistry;
use crate::storage::event_store::EventStore;
use crate::storage::{PageSize, RunCursor, RunListQuery, StateStore};
use crate::util::sensitive_url::redact_sensitive_text;

use super::ScheduleExecutor;
use super::config::ScheduleConfig;

/// Context key naming the schedule that triggered a run.
///
/// Underscore-prefixed, so it stays private when a child result is namespaced
/// (IF-057).
pub const SCHEDULE_CONTEXT_KEY: &str = "_schedule";

/// Most non-terminal runs the overlap check will inspect.
///
/// The scan only runs after an instant has been claimed — at most once per
/// schedule per cron instant, not once per 30-second tick — so the cost is
/// negligible in practice. The cap bounds the pathological case.
pub const MAX_OVERLAP_SCAN: usize = 256;

/// Runs schedules through the same engine path the REST API uses.
pub struct FlowExecutor {
    registry: Arc<NodeRegistry>,
    store: Arc<dyn StateStore>,
    event_store: Arc<dyn EventStore>,
    flows_dir: Option<PathBuf>,
    max_concurrent_tasks: Option<usize>,
}

impl FlowExecutor {
    pub fn new(
        registry: Arc<NodeRegistry>,
        store: Arc<dyn StateStore>,
        event_store: Arc<dyn EventStore>,
        flows_dir: Option<PathBuf>,
        max_concurrent_tasks: Option<usize>,
    ) -> Self {
        Self {
            registry,
            store,
            event_store,
            flows_dir,
            max_concurrent_tasks,
        }
    }

    fn initial_context(
        &self,
        schedule_name: &str,
        schedule: &ScheduleConfig,
        path: &str,
    ) -> Context {
        let mut ctx = schedule.context().clone();
        ctx.insert(
            SCHEDULE_CONTEXT_KEY.to_string(),
            serde_json::Value::String(schedule_name.to_string()),
        );
        if let Some(dir) = std::path::Path::new(path).parent() {
            ctx.insert(
                "_flow_dir".to_string(),
                serde_json::Value::String(dir.to_string_lossy().to_string()),
            );
        }
        ctx
    }
}

#[async_trait]
impl ScheduleExecutor for FlowExecutor {
    async fn active_run(&self, schedule_name: &str) -> Option<String> {
        let page_size = match PageSize::new(64) {
            Ok(page_size) => page_size,
            Err(error) => {
                tracing::warn!(
                    schedule = %schedule_name,
                    %error,
                    "overlap check could not build its page size; \
                     a still-running previous run may have been missed"
                );
                return None;
            }
        };

        let mut examined = 0;
        for status in [RunStatus::Pending, RunStatus::Running] {
            let mut after: Option<RunCursor> = None;
            loop {
                let query = match RunListQuery::new(Some(status.clone()), after, page_size) {
                    Ok(query) => query,
                    Err(error) => {
                        tracing::warn!(
                            schedule = %schedule_name,
                            %error,
                            "overlap check could not build its list query; \
                             a still-running previous run may have been missed"
                        );
                        return None;
                    }
                };
                let page = match self.store.list_run_summaries_page(&query).await {
                    Ok(page) => page,
                    Err(error) => {
                        tracing::warn!(
                            schedule = %schedule_name,
                            %error,
                            "overlap check failed to list runs; \
                             a still-running previous run may have been missed"
                        );
                        return None;
                    }
                };

                for summary in &page.items {
                    if examined >= MAX_OVERLAP_SCAN {
                        tracing::warn!(
                            schedule = %schedule_name,
                            limit = MAX_OVERLAP_SCAN,
                            "overlap check stopped at its scan limit; \
                             a still-running previous run may have been missed"
                        );
                        return None;
                    }
                    examined += 1;

                    // The schedule name lives in the run's context, so the
                    // summary alone cannot answer this.
                    let info = match self.store.get_run_info(&summary.id).await {
                        Ok(info) => info,
                        Err(error) => {
                            tracing::debug!(
                                run_id = %summary.id,
                                %error,
                                "overlap check could not read a run's info; skipping it"
                            );
                            continue;
                        }
                    };
                    if info.ctx.get(SCHEDULE_CONTEXT_KEY).and_then(|v| v.as_str())
                        == Some(schedule_name)
                    {
                        return Some(summary.id.clone());
                    }
                }

                match page.next {
                    Some(cursor) => after = Some(cursor),
                    None => break,
                }
            }
        }
        None
    }

    fn has_capacity(&self) -> bool {
        // The permit is dropped immediately; this is a probe, not a
        // reservation. `run` acquires the permit it actually holds.
        crate::api::acquire_run_permit().is_ok()
    }

    async fn run(&self, schedule_name: &str, schedule: &ScheduleConfig) -> Result<String, String> {
        let path = resolve_flow_path_in(schedule.flow(), self.flows_dir.as_deref())
            .map_err(|error| format!("{error:?}"))?;

        // Held for the run's real duration: moved into the spawned task below,
        // not dropped when this function returns. Reserve it before the Lua VM
        // is created so a run rejected for capacity cannot still consume parse
        // CPU and memory.
        let run_permit = crate::api::acquire_run_permit()
            .map_err(|_| "server is at maximum concurrent run capacity".to_string())?;
        let flow_load_permit = crate::api::acquire_flow_load_permit()
            .map_err(|_| "server is at maximum concurrent flow-loading capacity".to_string())?;

        // Parse off the async runtime so a pathological flow cannot pin a
        // worker thread and stall the server (IF-038). The error can echo
        // file-derived tokens (`near '<token>'`), so it is redacted exactly as
        // the REST API redacts the same failure (see `helpers::flow_file_load_error`)
        // before it becomes a string that gets logged.
        let registry = self.registry.clone();
        let load_path = path.clone();
        let flow = crate::api::supervise_flow_load(flow_load_permit, async move {
            LuaRuntime::load_flow_async(&load_path, &registry).await
        })
        .await
        .map_err(|error| {
            format!(
                "failed to load flow: {}",
                redact_sensitive_text(&format!("{error:#}"))
            )
        })?;

        let initial_ctx = self.initial_context(schedule_name, schedule, &path);

        let engine = WorkflowEngine::new_with_events(
            self.registry.clone(),
            self.store.clone(),
            self.event_store.clone(),
            self.max_concurrent_tasks,
        );

        // Start and return once the run exists, without awaiting it to
        // completion. The tick loop awaits `evaluate`, which awaits `decide`,
        // which awaits this: if this awaited the whole run, one long-running
        // flow would starve every other schedule's next tick, and a flow with
        // no step deadline that never returns would silently end all
        // scheduling for the process's lifetime. `RunHandle::wait` retains
        // detach semantics, which is exactly what running it in a background
        // task needs.
        let handle = engine
            .start(&flow, initial_ctx)
            .await
            .map_err(|error| format!("{error:#}"))?;
        let run_id = handle.id().to_string();
        let waited_run_id = run_id.clone();
        let schedule_name = schedule_name.to_string();

        tokio::spawn(async move {
            // Held until the run finishes, exactly as the API does for a
            // synchronous run (IF-042); the run outliving `run()` is the point
            // of this change, so the permit must too.
            let _run_permit = run_permit;
            if let Err(error) = handle.wait().await {
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

        Ok(run_id)
    }
}
