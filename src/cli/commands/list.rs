use std::sync::Arc;

use anyhow::Result;

use crate::storage::StateStore;

pub(crate) async fn cmd_list(
    status_filter: Option<String>,
    store: Arc<dyn StateStore>,
    format: String,
) -> Result<()> {
    let status = status_filter
        .as_deref()
        .map(|s| match s {
            "pending" => Ok(crate::engine::types::RunStatus::Pending),
            "running" => Ok(crate::engine::types::RunStatus::Running),
            "success" => Ok(crate::engine::types::RunStatus::Success),
            "failed" => Ok(crate::engine::types::RunStatus::Failed),
            "stalled" => Ok(crate::engine::types::RunStatus::Stalled),
            _ => Err(anyhow::anyhow!("Invalid status filter: {}", s)),
        })
        .transpose()?;

    let runs = store.list_runs(status).await?;

    if runs.is_empty() {
        println!("No runs found.");
        return Ok(());
    }

    if format == "json" {
        println!("{}", serde_json::to_string_pretty(&runs)?);
        return Ok(());
    }

    // Table format
    println!(
        "{:<38} {:<20} {:<10} {:<24}",
        "RUN ID", "FLOW", "STATUS", "STARTED"
    );
    println!("{}", "-".repeat(92));

    for run in &runs {
        let started = run
            .started
            .map(|t| t.format("%Y-%m-%d %H:%M:%S").to_string())
            .unwrap_or_else(|| "-".to_string());

        println!(
            "{:<38} {:<20} {:<10} {:<24}",
            run.id, run.flow_name, run.status, started
        );
    }

    println!("\nTotal: {} run(s)", runs.len());
    Ok(())
}
