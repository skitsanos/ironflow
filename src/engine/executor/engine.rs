use std::sync::Arc;

use anyhow::{Context as _, Result, bail};
use tracing::{warn, warn_span};
use uuid::Uuid;

use crate::engine::events::RunEvent;
use crate::engine::types::{Context, FlowDefinition};
use crate::nodes::NodeRegistry;
use crate::storage::event_store::EventStore;
use crate::storage::{RunLease, StateStore};

use super::coordinator::{RunCoordinator, RunHandle};
use super::overlay::ExecutionOverlay;

const EVENT_PUBLISH_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(500);

/// The core workflow execution engine.
pub struct WorkflowEngine {
    pub(super) registry: Arc<NodeRegistry>,
    pub(super) store: Arc<dyn StateStore>,
    pub(super) events: Option<Arc<dyn EventStore>>,
    pub(super) max_concurrent_tasks: Option<usize>,
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
            max_concurrent_tasks,
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
            max_concurrent_tasks,
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
        // Resolve limits before validating the flow or creating durable state.
        // Embedded callers receive the same fail-closed behavior as the CLI.
        let max_concurrent_tasks =
            crate::util::runtime_config::max_concurrent_tasks(self.max_concurrent_tasks)?;
        let run_deadline = crate::util::runtime_config::run_deadline()?;
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
        let lease = RunLease::fresh();
        let durable_initial_ctx = execution_overlay.redact_context(&initial_ctx);
        super::lease::initialize_run(
            self.store.as_ref(),
            &run_id,
            &flow.name,
            durable_initial_ctx.as_ref(),
            &lease,
        )
        .await
        .with_context(|| format!("Failed to initialize workflow run {run_id}"))?;

        let coordinator = RunCoordinator::new(
            self.registry.clone(),
            self.store.clone(),
            self.events.clone(),
            max_concurrent_tasks,
            run_id,
            flow.clone(),
            execution_plan,
            initial_ctx,
            execution_overlay,
            lease.owner().to_string(),
            run_deadline,
        );

        Ok(coordinator.spawn())
    }

    /// Execute a flow definition and wait for its supervised run to finish.
    pub async fn execute(&self, flow: &FlowDefinition, initial_ctx: Context) -> Result<String> {
        self.start(flow, initial_ctx).await?.wait().await
    }

    pub(super) async fn publish_event_ref(events: Option<&Arc<dyn EventStore>>, event: RunEvent) {
        let Some(events) = events else {
            return;
        };
        match tokio::time::timeout(EVENT_PUBLISH_TIMEOUT, events.publish(event)).await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                let _span = warn_span!("workflow_event_publish").entered();
                warn!(error = %error, "Failed to publish workflow event");
            }
            Err(_) => {
                let _span = warn_span!("workflow_event_publish").entered();
                warn!(
                    timeout_ms = EVENT_PUBLISH_TIMEOUT.as_millis(),
                    "Workflow event publication timed out"
                );
            }
        }
    }
}

#[cfg(test)]
mod tests;
