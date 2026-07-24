use anyhow::Result;
use chrono::Utc;
use tracing::{info, warn};

use crate::engine::events::{RunEvent, RunEventType};
use crate::engine::types::{RunStatus, TaskState, TaskStatus};

use super::coordinator::{ExecutionOutcome, RunCoordinator};
use super::engine::WorkflowEngine;

const TERMINAL_STATUS_ATTEMPTS: usize = 3;

impl RunCoordinator {
    pub(super) async fn finalize(&self, outcome: ExecutionOutcome) -> Result<()> {
        let mut status = match &outcome {
            ExecutionOutcome::Completed(status) => status.clone(),
            ExecutionOutcome::Cancelled => RunStatus::Cancelled,
            ExecutionOutcome::Infrastructure(_) => RunStatus::Stalled,
        };
        let mut errors = Vec::new();
        if let ExecutionOutcome::Infrastructure(error) = &outcome {
            errors.push(format!("execution failed: {error:#}"));
        }

        let final_ctx = self.ctx.read().await.clone();
        let durable_final_ctx =
            super::output::bound_context(self.execution_overlay.redact_context(final_ctx.as_ref()));
        match self
            .store
            .update_ctx(&self.run_id, &durable_final_ctx)
            .await
        {
            Ok(()) => {
                // The final context is persisted during terminalization, so
                // stamp the event with the resolved terminal status rather than
                // a misleading `Running` (IF-052).
                WorkflowEngine::publish_event_ref(
                    self.events.as_ref(),
                    RunEvent::run(
                        &self.run_id,
                        &self.flow.name,
                        RunEventType::ContextUpdated,
                        status.clone(),
                    ),
                )
                .await;
            }
            Err(error) => {
                status = RunStatus::Stalled;
                errors.push(format!("final context persistence failed: {error:#}"));
            }
        }

        let run_info = match self.store.get_run_info(&self.run_id).await {
            Ok(info) => Some(info),
            Err(error) => {
                status = RunStatus::Stalled;
                errors.push(format!("task-state inspection failed: {error:#}"));
                None
            }
        };

        if let Some(info) = &run_info
            && matches!(status, RunStatus::Success | RunStatus::Failed)
            && info.tasks.values().any(|task| !task.status.is_terminal())
        {
            status = RunStatus::Stalled;
            errors.push("execution ended with non-terminal task state".to_string());
        }

        if let Some(info) = run_info {
            for task in info.tasks.values() {
                if let Err(error) = self.repair_task(task, &status).await {
                    status = RunStatus::Stalled;
                    errors.push(format!(
                        "failed to terminalize task '{}': {error:#}",
                        task.name
                    ));
                }
            }
        }

        if let Err(error) = self.persist_terminal_status(status.clone()).await {
            errors.push(format!("terminal status persistence failed: {error:#}"));
            return Err(anyhow::anyhow!(
                "Run '{}' could not be terminalized: {}",
                self.run_id,
                errors.join("; ")
            ));
        }

        let public_reason = match status {
            RunStatus::Cancelled => Some("workflow execution was cancelled"),
            RunStatus::Stalled => {
                Some("workflow execution stopped after an infrastructure failure")
            }
            _ => None,
        };
        let mut event = RunEvent::run(
            &self.run_id,
            &self.flow.name,
            RunEventType::RunFinished,
            status.clone(),
        );
        if let Some(reason) = public_reason {
            event = event.with_reason(reason);
        }
        WorkflowEngine::publish_event_ref(self.events.as_ref(), event).await;

        info!(run_id = %self.run_id, status = %status, "Workflow execution complete");

        if status == RunStatus::Stalled {
            Err(anyhow::anyhow!(
                "Run '{}' stalled: {}",
                self.run_id,
                errors.join("; ")
            ))
        } else {
            Ok(())
        }
    }

    async fn repair_task(&self, existing: &TaskState, run_status: &RunStatus) -> Result<()> {
        if existing.status.is_terminal() && existing.finished.is_some() {
            return Ok(());
        }

        let mut task = existing.clone();
        let status_changed = !task.status.is_terminal();
        if status_changed {
            task.status = match run_status {
                RunStatus::Cancelled => TaskStatus::Cancelled,
                _ if existing.status == TaskStatus::Running => TaskStatus::Failed,
                _ => TaskStatus::Skipped,
            };
            task.error = Some(match run_status {
                RunStatus::Cancelled => "workflow execution was cancelled".to_string(),
                _ => "task stopped before the workflow could complete".to_string(),
            });
        }
        task.finished = Some(Utc::now());

        self.store.upsert_task(&self.run_id, &task).await?;

        if status_changed {
            let event_type = match task.status {
                TaskStatus::Cancelled => RunEventType::TaskCancelled,
                TaskStatus::Failed => RunEventType::TaskFailed,
                _ => RunEventType::TaskSkipped,
            };
            WorkflowEngine::publish_event_ref(
                self.events.as_ref(),
                RunEvent::task(
                    &self.run_id,
                    &task.name,
                    &task.node_type,
                    event_type,
                    task.status.clone(),
                    (task.attempt > 0).then_some(task.attempt),
                )
                .with_reason(task.error.clone().unwrap_or_default()),
            )
            .await;
        }

        Ok(())
    }

    async fn persist_terminal_status(&self, status: RunStatus) -> Result<()> {
        let mut last_error = None;
        for attempt in 1..=TERMINAL_STATUS_ATTEMPTS {
            match self
                .store
                .set_run_status(&self.run_id, status.clone())
                .await
            {
                Ok(()) => return Ok(()),
                Err(error) => {
                    if let Ok(info) = self.store.get_run_info(&self.run_id).await
                        && info.status == status
                        && info.finished.is_some()
                    {
                        return Ok(());
                    }
                    warn!(
                        run_id = %self.run_id,
                        attempt,
                        error = %error,
                        "Failed to persist terminal workflow status"
                    );
                    last_error = Some(error);
                    tokio::task::yield_now().await;
                }
            }
        }

        Err(last_error
            .map(anyhow::Error::new)
            .unwrap_or_else(|| anyhow::anyhow!("terminal status write failed")))
    }
}
