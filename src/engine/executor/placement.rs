//! Per-phase placement: which members run, and which are terminalized as
//! skipped because a dependency is unavailable or a route did not match.

use std::sync::Arc;

use anyhow::Context as _;
use chrono::Utc;
use tokio::sync::RwLock;
use tracing::{info, warn};

use crate::engine::events::{RunEvent, RunEventType};
use crate::engine::types::{Context, StepDefinition, TaskState, TaskStatus};

use super::coordinator::RunCoordinator;
use super::engine::WorkflowEngine;
use super::error_handler::{ExecutionState, RecoveryInvocation};
use super::workflow::{Progress, infrastructure};

/// A phase member either runs, optionally as a recovery handler, or was
/// already terminalized as skipped in durable state.
pub(super) enum Placement {
    Run(Option<RecoveryInvocation>),
    Skipped,
}

impl RunCoordinator {
    /// Decide whether one phase member runs. Members with an unavailable
    /// dependency or an unmatched route are terminalized as skipped instead.
    pub(super) async fn place_step(
        &self,
        step: &Arc<StepDefinition>,
        state: &RwLock<ExecutionState>,
        phase_ctx: &Context,
    ) -> Progress<Placement> {
        if let Some(source) = self.execution_plan.recovery_sources.get(&step.name) {
            return self.place_recovery_handler(step, source, state).await;
        }

        let blocked_dependency = state.read().await.blocked_dependency(&step.dependencies);
        if let Some(dependency) = blocked_dependency {
            warn!(
                task = %step.name,
                dependency = %dependency,
                "Skipping task because a dependency is unavailable"
            );
            return self
                .skip_unavailable(step, state, "dependency failed")
                .await;
        }

        if let Some(route) = &step.route
            && !WorkflowEngine::check_route(step, route, phase_ctx)
        {
            info!(task = %step.name, route = %route, "Skipping task because its route did not match");
            self.mark_skipped(step, "route condition was not matched")
                .await
                .map_err(infrastructure)?;
            return Ok(Placement::Skipped);
        }

        Ok(Placement::Run(None))
    }

    /// A recovery handler runs only when its source step failed and none of
    /// its own dependencies are unavailable.
    async fn place_recovery_handler(
        &self,
        step: &Arc<StepDefinition>,
        source: &str,
        state: &RwLock<ExecutionState>,
    ) -> Progress<Placement> {
        let source_failure = state.write().await.take_for_recovery(source);
        let Some(failure) = source_failure else {
            return self
                .skip_unavailable(step, state, "error handler was not triggered")
                .await;
        };

        let blocked_dependency = state.read().await.blocked_dependency(&step.dependencies);
        if let Some(dependency) = blocked_dependency {
            warn!(
                task = %step.name,
                dependency = %dependency,
                "Skipping recovery handler because a dependency is unavailable"
            );
            return self
                .skip_unavailable(step, state, "recovery handler dependency failed")
                .await;
        }

        Ok(Placement::Run(Some(RecoveryInvocation {
            source: source.to_string(),
            failure,
        })))
    }

    async fn skip_unavailable(
        &self,
        step: &StepDefinition,
        state: &RwLock<ExecutionState>,
        reason: &str,
    ) -> Progress<Placement> {
        self.mark_skipped(step, reason)
            .await
            .map_err(infrastructure)?;
        state.write().await.mark_unavailable(&step.name);
        Ok(Placement::Skipped)
    }

    async fn mark_skipped(&self, step: &StepDefinition, reason: &str) -> anyhow::Result<()> {
        let mut task_state = TaskState::new(&step.name, &step.node_type);
        task_state.status = TaskStatus::Skipped;
        task_state.finished = Some(Utc::now());
        super::lease::persist_task(
            self.store.as_ref(),
            &self.run_id,
            &task_state,
            &self.lease_owner,
        )
        .await
        .with_context(|| format!("Failed to persist task '{}' as skipped", step.name))?;
        WorkflowEngine::publish_event_ref(
            self.events.as_ref(),
            RunEvent::task(
                &self.run_id,
                &step.name,
                &step.node_type,
                RunEventType::TaskSkipped,
                TaskStatus::Skipped,
                None,
            )
            .with_reason(reason),
        )
        .await;
        Ok(())
    }
}
