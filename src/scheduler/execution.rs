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
        let Ok(page_size) = PageSize::new(64) else {
            return None;
        };

        let mut examined = 0;
        for status in [RunStatus::Pending, RunStatus::Running] {
            let mut after: Option<RunCursor> = None;
            loop {
                let Ok(query) = RunListQuery::new(Some(status.clone()), after, page_size) else {
                    return None;
                };
                let Ok(page) = self.store.list_run_summaries_page(&query).await else {
                    return None;
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
                    let Ok(info) = self.store.get_run_info(&summary.id).await else {
                        continue;
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

        // Parse off the async runtime so a pathological flow cannot pin a
        // worker thread and stall the server (IF-038).
        let flow = LuaRuntime::load_flow_async(&path, &self.registry)
            .await
            .map_err(|error| format!("failed to load flow: {error:#}"))?;

        let initial_ctx = self.initial_context(schedule_name, schedule, &path);

        // Held until the run finishes, exactly as the API does (IF-042).
        let _run_permit = crate::api::acquire_run_permit()
            .map_err(|_| "server is at maximum concurrent run capacity".to_string())?;

        let engine = WorkflowEngine::new_with_events(
            self.registry.clone(),
            self.store.clone(),
            self.event_store.clone(),
            self.max_concurrent_tasks,
        );
        engine
            .execute(&flow, initial_ctx)
            .await
            .map_err(|error| format!("{error:#}"))
    }
}
