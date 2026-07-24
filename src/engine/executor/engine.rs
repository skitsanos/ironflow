use std::sync::Arc;

use anyhow::{Context as _, Result, bail};
use tracing::{warn, warn_span};
use uuid::Uuid;

use crate::engine::events::RunEvent;
use crate::engine::types::{Context, FlowDefinition};
use crate::nodes::NodeRegistry;
use crate::storage::StateStore;
use crate::storage::event_store::EventStore;

use super::coordinator::{RunCoordinator, RunHandle};
use super::overlay::ExecutionOverlay;

/// The core workflow execution engine.
pub struct WorkflowEngine {
    pub(super) registry: Arc<NodeRegistry>,
    pub(super) store: Arc<dyn StateStore>,
    pub(super) events: Option<Arc<dyn EventStore>>,
    pub(super) max_concurrent_tasks: usize,
}

fn resolve_max_concurrent_tasks(configured: Option<usize>) -> usize {
    let value = configured
        .or_else(|| {
            std::env::var("IRONFLOW_MAX_CONCURRENT_TASKS")
                .ok()
                .and_then(|value| value.parse().ok())
        })
        .unwrap_or_else(num_cpus::get);

    if value == 0 {
        warn!("max_concurrent_tasks=0 would deadlock execution; using 1");
        1
    } else {
        value
    }
}

impl WorkflowEngine {
    pub fn new(
        registry: Arc<NodeRegistry>,
        store: Arc<dyn StateStore>,
        max_concurrent_tasks: Option<usize>,
    ) -> Self {
        Self {
            registry,
            store,
            events: None,
            max_concurrent_tasks: resolve_max_concurrent_tasks(max_concurrent_tasks),
        }
    }

    pub fn new_with_events(
        registry: Arc<NodeRegistry>,
        store: Arc<dyn StateStore>,
        events: Arc<dyn EventStore>,
        max_concurrent_tasks: Option<usize>,
    ) -> Self {
        Self {
            registry,
            store,
            events: Some(events),
            max_concurrent_tasks: resolve_max_concurrent_tasks(max_concurrent_tasks),
        }
    }

    /// Start a supervised workflow run and return its durable handle.
    ///
    /// Dropping the handle only detaches the waiter; the coordinator keeps
    /// running and terminalizes the run. Call [`RunHandle::cancel`] for an
    /// explicit, persisted cancellation.
    pub async fn start(&self, flow: &FlowDefinition, initial_ctx: Context) -> Result<RunHandle> {
        self.start_with_execution_overlay(flow, initial_ctx, ExecutionOverlay::default())
            .await
    }

    pub(crate) async fn start_with_overlay(
        &self,
        flow: &FlowDefinition,
        initial_ctx: Context,
        overlay: Context,
    ) -> Result<RunHandle> {
        self.start_with_execution_overlay(flow, initial_ctx, ExecutionOverlay::new(overlay))
            .await
    }

    pub(crate) async fn start_with_execution_overlay(
        &self,
        flow: &FlowDefinition,
        initial_ctx: Context,
        execution_overlay: ExecutionOverlay,
    ) -> Result<RunHandle> {
        let execution_plan = self.execution_plan(flow)?;
        let unknown_nodes: Vec<String> = flow
            .steps
            .iter()
            .filter(|step| self.registry.get(&step.node_type).is_none())
            .map(|step| format!("'{}' ({})", step.name, step.node_type))
            .collect();
        if !unknown_nodes.is_empty() {
            bail!(
                "Flow contains steps with unknown node types: {}",
                unknown_nodes.join(", ")
            );
        }

        let run_id = Uuid::new_v4().to_string();
        let durable_initial_ctx = execution_overlay.redact_context(&initial_ctx);
        self.store
            .init_run(&run_id, &flow.name, &durable_initial_ctx)
            .await
            .with_context(|| format!("Failed to initialize workflow run {run_id}"))?;

        let coordinator = RunCoordinator::new(
            self.registry.clone(),
            self.store.clone(),
            self.events.clone(),
            self.max_concurrent_tasks,
            run_id,
            flow.clone(),
            execution_plan,
            initial_ctx,
            execution_overlay,
        );

        Ok(coordinator.spawn())
    }

    /// Execute a flow definition and wait for its supervised run to finish.
    pub async fn execute(&self, flow: &FlowDefinition, initial_ctx: Context) -> Result<String> {
        self.start(flow, initial_ctx).await?.wait().await
    }

    pub(crate) async fn execute_with_overlay(
        &self,
        flow: &FlowDefinition,
        initial_ctx: Context,
        overlay: Context,
    ) -> Result<String> {
        self.start_with_overlay(flow, initial_ctx, overlay)
            .await?
            .wait()
            .await
    }

    pub(super) async fn publish_event_ref(events: Option<&Arc<dyn EventStore>>, event: RunEvent) {
        if let Some(events) = events
            && let Err(error) = events.publish(event).await
        {
            let _span = warn_span!("workflow_event_publish").entered();
            warn!(error = %error, "Failed to publish workflow event");
        }
    }
}
