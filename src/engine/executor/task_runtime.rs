use std::sync::Arc;

use crate::engine::types::{Context, TaskState};
use crate::nodes::NodeRegistry;
use crate::storage::StateStore;
use crate::storage::event_store::EventStore;

use super::overlay::ExecutionOverlay;

/// Dependencies and run-scoped state used by one task execution.
pub(super) struct TaskRuntime<'a> {
    pub(super) registry: &'a NodeRegistry,
    pub(super) store: &'a Arc<dyn StateStore>,
    pub(super) events: Option<&'a Arc<dyn EventStore>>,
    pub(super) run_id: &'a str,
    pub(super) phase_ctx: &'a Arc<Context>,
    pub(super) execution_overlay: &'a ExecutionOverlay,
    pub(super) lease_owner: &'a str,
    pub(super) metrics: Option<&'a Arc<crate::metrics::Metrics>>,
}

impl<'a> TaskRuntime<'a> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        registry: &'a NodeRegistry,
        store: &'a Arc<dyn StateStore>,
        events: Option<&'a Arc<dyn EventStore>>,
        run_id: &'a str,
        phase_ctx: &'a Arc<Context>,
        execution_overlay: &'a ExecutionOverlay,
        lease_owner: &'a str,
        metrics: Option<&'a Arc<crate::metrics::Metrics>>,
    ) -> Self {
        Self {
            registry,
            store,
            events,
            run_id,
            phase_ctx,
            execution_overlay,
            lease_owner,
            metrics,
        }
    }

    pub(super) async fn persist_task(&self, task: &TaskState) -> crate::storage::StorageResult<()> {
        super::lease::persist_task(self.store.as_ref(), self.run_id, task, self.lease_owner).await
    }
}
