use std::sync::Arc;

use anyhow::{Context as _, Result, bail};
use chrono::Utc;
use tokio::sync::RwLock;
use tracing::{info, warn};

use crate::engine::events::{RunEvent, RunEventType};
use crate::engine::types::{Context, StepDefinition, TaskState, TaskStatus};
use crate::nodes::NodeRegistry;
use crate::storage::StateStore;
use crate::storage::event_store::EventStore;

use super::context::task_duration_ms;
use super::engine::WorkflowEngine;

impl WorkflowEngine {
    /// Run a single task with retry logic.
    pub(super) async fn run_task(
        registry: &NodeRegistry,
        store: &Arc<dyn StateStore>,
        events: Option<&Arc<dyn EventStore>>,
        run_id: &str,
        step: &StepDefinition,
        ctx: &Arc<RwLock<Arc<Context>>>,
    ) -> Result<()> {
        let node = registry
            .get(&step.node_type)
            .with_context(|| format!("Unknown node type: {}", step.node_type))?;

        let max_attempts = step.retry.max_retries + 1;
        let mut last_error = None;

        for attempt in 1..=max_attempts {
            // Update task state to running
            let mut task_state = TaskState::new(&step.name, &step.node_type);
            task_state.status = TaskStatus::Running;
            task_state.attempt = attempt;
            task_state.started = Some(Utc::now());
            store.upsert_task(run_id, &task_state).await?;
            Self::publish_event_ref(
                events,
                RunEvent::task(
                    run_id,
                    &step.name,
                    &step.node_type,
                    RunEventType::TaskStarted,
                    TaskStatus::Running,
                    Some(attempt),
                ),
            )
            .await;

            info!(task = %step.name, attempt = attempt, max = max_attempts, "Running task");

            // Cheap snapshot — `Arc::clone` of the context pointer. The node
            // borrows from the pointed-to `Context`; writers make-mut to a
            // fresh Arc so this snapshot stays stable for the call.
            let current_ctx: Arc<Context> = ctx.read().await.clone();

            let result = if let Some(timeout_s) = step.timeout_s {
                let duration = std::time::Duration::from_secs_f64(timeout_s);
                match tokio::time::timeout(duration, node.execute(&step.config, &current_ctx)).await
                {
                    Ok(r) => r,
                    Err(_) => Err(anyhow::anyhow!("Task timed out after {}s", timeout_s)),
                }
            } else {
                node.execute(&step.config, &current_ctx).await
            };

            match result {
                Ok(output) => {
                    // Merge output into context. `Arc::make_mut` clones the
                    // inner HashMap only when it's shared with a live reader;
                    // once cloned, future writes go in-place until the next
                    // reader snapshot.
                    {
                        let mut ctx_write = ctx.write().await;
                        let inner = Arc::make_mut(&mut *ctx_write);
                        for (k, v) in &output {
                            inner.insert(k.clone(), v.clone());
                        }
                    }

                    // Update task state to success. `output` is a
                    // HashMap<String, Value> — convert it to a JSON object
                    // directly instead of going through `serde_json::to_value`,
                    // which would walk every Value through the Serialize trait
                    // even though each element is already a Value.
                    task_state.status = TaskStatus::Success;
                    let output_value = serde_json::Value::Object(
                        output.into_iter().collect::<serde_json::Map<_, _>>(),
                    );
                    // Cap what we persist in task history — huge outputs
                    // already landed in `ctx` via the merge above; there's
                    // no need to duplicate them in the run record.
                    let max_task_bytes = crate::util::limits::max_task_output_bytes() as usize;
                    let serialized_size = output_value.to_string().len();
                    task_state.output = if serialized_size > max_task_bytes {
                        Some(serde_json::json!({
                            "_truncated": true,
                            "_original_bytes": serialized_size,
                            "_limit_bytes": max_task_bytes,
                            "_note": "Output exceeded IRONFLOW_MAX_TASK_OUTPUT_BYTES; full value is in workflow context.",
                        }))
                    } else {
                        Some(output_value)
                    };
                    task_state.finished = Some(Utc::now());
                    let duration_ms = task_duration_ms(task_state.started, task_state.finished);
                    store.upsert_task(run_id, &task_state).await?;
                    Self::publish_event_ref(
                        events,
                        RunEvent::task(
                            run_id,
                            &step.name,
                            &step.node_type,
                            RunEventType::TaskSucceeded,
                            TaskStatus::Success,
                            Some(attempt),
                        )
                        .with_duration_ms(duration_ms),
                    )
                    .await;

                    info!(task = %step.name, "Task completed successfully");
                    return Ok(());
                }
                Err(e) => {
                    let err_msg = format!("{:#}", e);
                    warn!(task = %step.name, attempt = attempt, error = %err_msg, "Task attempt failed");

                    task_state.status = TaskStatus::Failed;
                    task_state.error = Some(err_msg.clone());
                    task_state.finished = Some(Utc::now());
                    let duration_ms = task_duration_ms(task_state.started, task_state.finished);
                    store.upsert_task(run_id, &task_state).await?;
                    Self::publish_event_ref(
                        events,
                        RunEvent::task(
                            run_id,
                            &step.name,
                            &step.node_type,
                            RunEventType::TaskFailed,
                            TaskStatus::Failed,
                            Some(attempt),
                        )
                        .with_duration_ms(duration_ms)
                        .with_error(err_msg.clone()),
                    )
                    .await;

                    last_error = Some(err_msg);

                    // Apply backoff before retry (unless this was the last attempt)
                    if attempt < max_attempts {
                        let delay = step.retry.backoff_s * 2.0_f64.powi((attempt - 1) as i32);
                        info!(task = %step.name, delay_s = delay, "Retrying after backoff");
                        Self::publish_event_ref(
                            events,
                            RunEvent::task(
                                run_id,
                                &step.name,
                                &step.node_type,
                                RunEventType::TaskRetrying,
                                TaskStatus::Running,
                                Some(attempt + 1),
                            ),
                        )
                        .await;
                        tokio::time::sleep(std::time::Duration::from_secs_f64(delay)).await;
                    }
                }
            }
        }

        bail!(
            "Task '{}' failed after {} attempts: {}",
            step.name,
            max_attempts,
            last_error.unwrap_or_default()
        )
    }
}
