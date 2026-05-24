use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context as _, Result};

use crate::engine::WorkflowEngine;
use crate::engine::types::Context;
use crate::lua::LuaRuntime;
use crate::nodes::NodeRegistry;
use crate::storage::StateStore;

pub(crate) async fn cmd_run(
    flow_path: PathBuf,
    context_json: Option<String>,
    verbose: bool,
    store: Arc<dyn StateStore>,
    max_concurrent_tasks: Option<usize>,
) -> Result<()> {
    let registry = NodeRegistry::with_builtins();

    let flow_str = flow_path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("Invalid flow path"))?;

    let flow = LuaRuntime::load_flow(flow_str, &registry)
        .with_context(|| format!("Failed to load flow: {}", flow_path.display()))?;

    println!("Flow: {} ({} steps)", flow.name, flow.steps.len());

    if verbose {
        println!("\nSteps:");
        for step in &flow.steps {
            let deps = if step.dependencies.is_empty() {
                String::from("none")
            } else {
                step.dependencies.join(", ")
            };
            println!("  {} [{}] deps: {}", step.name, step.node_type, deps);
            if step.retry.max_retries > 0 {
                println!(
                    "    retries: {}, backoff: {}s",
                    step.retry.max_retries, step.retry.backoff_s
                );
            }
            if let Some(t) = step.timeout_s {
                println!("    timeout: {}s", t);
            }
            if let Some(ref r) = step.route {
                println!("    route: {}", r);
            }
        }
    }

    // Parse initial context
    let mut initial_ctx: Context = match context_json {
        Some(json) => {
            serde_json::from_str(&json).with_context(|| "Failed to parse --context JSON")?
        }
        None => Context::new(),
    };

    // Inject _flow_dir so subworkflow nodes can resolve relative paths
    if let Some(flow_dir) = flow_path.canonicalize()?.parent() {
        initial_ctx.insert(
            "_flow_dir".to_string(),
            serde_json::Value::String(flow_dir.to_string_lossy().to_string()),
        );
    }

    let engine = WorkflowEngine::new(Arc::new(registry), store.clone(), max_concurrent_tasks);

    let run_id = engine.execute(&flow, initial_ctx).await?;

    // Print results
    let run_info = store.get_run_info(&run_id).await?;
    println!("\nRun ID: {}", run_id);
    println!("Status: {}", run_info.status);

    println!("\nTasks:");
    for (name, task) in &run_info.tasks {
        let status_icon = match task.status {
            crate::engine::types::TaskStatus::Success => "✓",
            crate::engine::types::TaskStatus::Failed => "✗",
            crate::engine::types::TaskStatus::Skipped => "⊘",
            crate::engine::types::TaskStatus::Running => "⟳",
            crate::engine::types::TaskStatus::Pending => "○",
        };
        println!(
            "  {} {} [{}] (attempt {})",
            status_icon, name, task.node_type, task.attempt
        );
        if verbose && let (Some(s), Some(f)) = (&task.started, &task.finished) {
            let duration = *f - *s;
            println!("    Duration: {}ms", duration.num_milliseconds());
        }
        if let Some(ref err) = task.error {
            println!("    Error: {}", err);
        }
        if verbose && let Some(ref output) = task.output {
            println!("    Output: {}", output);
        }
    }

    if !run_info.ctx.is_empty() {
        // Only print non-internal context keys
        let user_ctx: Context = run_info
            .ctx
            .iter()
            .filter(|(k, _)| !k.starts_with('_'))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        if !user_ctx.is_empty() {
            println!("\nContext:");
            println!("{}", serde_json::to_string_pretty(&user_ctx)?);
        }
    }

    Ok(())
}
