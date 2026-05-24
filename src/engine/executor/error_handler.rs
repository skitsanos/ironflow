use std::collections::HashSet;
use std::sync::Arc;

use tokio::sync::RwLock;
use tracing::error;

use crate::engine::types::{Context, StepDefinition};
use crate::nodes::NodeRegistry;
use crate::storage::StateStore;
use crate::storage::event_store::EventStore;

use super::engine::WorkflowEngine;

impl WorkflowEngine {
    /// Handle an error for a step that has an `on_error` handler configured.
    /// Injects `_error_message`, `_error_step`, `_error_node_type` into context,
    /// runs the handler step, and updates `completed`/`failed`/`error_handled` sets.
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn handle_step_error(
        registry: &NodeRegistry,
        store: &Arc<dyn StateStore>,
        events: Option<&Arc<dyn EventStore>>,
        run_id: &str,
        step: &Arc<StepDefinition>,
        step_map: &Arc<std::collections::HashMap<String, Arc<StepDefinition>>>,
        ctx: &Arc<RwLock<Arc<Context>>>,
        completed: &Arc<RwLock<HashSet<String>>>,
        failed: &Arc<RwLock<HashSet<String>>>,
        error_handled: &Arc<RwLock<HashSet<String>>>,
        e: anyhow::Error,
    ) {
        let error_step_name = match &step.on_error {
            Some(name) => name.clone(),
            None => {
                error!(task = %step.name, error = %e, "Task failed");
                failed.write().await.insert(step.name.clone());
                return;
            }
        };

        tracing::warn!(
            task = %step.name,
            error_handler = %error_step_name,
            "Task failed — routing to error handler"
        );

        // Inject error details into context
        {
            let mut ctx_write = ctx.write().await;
            let inner = Arc::make_mut(&mut *ctx_write);
            inner.insert(
                "_error_message".to_string(),
                serde_json::Value::String(format!("{:#}", e)),
            );
            inner.insert(
                "_error_step".to_string(),
                serde_json::Value::String(step.name.clone()),
            );
            inner.insert(
                "_error_node_type".to_string(),
                serde_json::Value::String(step.node_type.clone()),
            );
        }

        // Run the error handler step
        if let Some(error_step) = step_map.get(&error_step_name) {
            let err_result = Self::run_task(registry, store, events, run_id, error_step, ctx).await;

            match err_result {
                Ok(()) => {
                    // Error was handled — mark original step as completed (error handled)
                    completed.write().await.insert(step.name.clone());
                    completed.write().await.insert(error_step_name.clone());
                    // Prevent the handler from running again in its normal phase
                    error_handled.write().await.insert(error_step_name.clone());
                }
                Err(handler_err) => {
                    error!(
                        task = %error_step_name,
                        error = %handler_err,
                        "Error handler also failed"
                    );
                    failed.write().await.insert(step.name.clone());
                }
            }
        } else {
            error!(
                task = %step.name,
                error_handler = %error_step_name,
                "Error handler step not found"
            );
            failed.write().await.insert(step.name.clone());
        }
    }
}
