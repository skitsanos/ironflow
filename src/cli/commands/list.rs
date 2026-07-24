use std::sync::Arc;

use anyhow::Result;

use crate::engine::types::RunStatus;
use crate::storage::{RunCursor, RunListQuery, StateStore};
use crate::util::listing::ListingPolicy;

#[derive(Clone, Copy)]
enum ListFormat {
    Table,
    Json,
}

pub(crate) struct PreparedList {
    query: RunListQuery,
    format: ListFormat,
}

pub(crate) fn prepare_list(
    status_filter: Option<String>,
    format: String,
    requested_limit: Option<usize>,
    after: Option<String>,
    listing_policy: ListingPolicy,
) -> Result<PreparedList> {
    let status = status_filter
        .as_deref()
        .map(|s| match s {
            "pending" => Ok(RunStatus::Pending),
            "running" => Ok(RunStatus::Running),
            "success" => Ok(RunStatus::Success),
            "failed" => Ok(RunStatus::Failed),
            "stalled" => Ok(RunStatus::Stalled),
            "cancelled" => Ok(RunStatus::Cancelled),
            _ => Err(anyhow::anyhow!("Invalid status filter: {}", s)),
        })
        .transpose()?;
    let format = match format.as_str() {
        "table" => ListFormat::Table,
        "json" => ListFormat::Json,
        _ => anyhow::bail!("Invalid output format: {format}. Use `table` or `json`"),
    };
    let limit = listing_policy.cli_page_size(requested_limit)?;
    let after = after.as_deref().map(RunCursor::decode).transpose()?;
    let query = RunListQuery::new(status, after, limit)?;
    Ok(PreparedList { query, format })
}

pub(crate) async fn cmd_list(store: Arc<dyn StateStore>, prepared: PreparedList) -> Result<()> {
    let PreparedList { query, format } = prepared;
    let limit = query.limit();
    let status_filter = query.status().map(ToString::to_string);
    let page = store.list_run_summaries_page(&query).await?;
    let returned = page.items.len();
    let has_more = page.has_more();
    let next_cursor = page.next.map(|cursor| cursor.encode()).transpose()?;

    if matches!(format, ListFormat::Json) {
        let output = serde_json::json!({
            "runs": page.items,
            "limit": limit.get(),
            "returned": returned,
            "has_more": has_more,
            "next_cursor": next_cursor,
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    if page.items.is_empty() {
        println!("No runs found.");
        return Ok(());
    }

    // Table format
    println!(
        "{:<38} {:<20} {:<10} {:<24}",
        "RUN ID", "FLOW", "STATUS", "STARTED"
    );
    println!("{}", "-".repeat(92));

    for run in &page.items {
        let started = run
            .started
            .map(|t| t.format("%Y-%m-%d %H:%M:%S").to_string())
            .unwrap_or_else(|| "-".to_string());

        println!(
            "{:<38} {:<20} {:<10} {:<24}",
            run.id, run.flow_name, run.status, started
        );
    }

    println!(
        "\nReturned: {returned} run(s) (page limit: {})",
        limit.get()
    );
    if let Some(next_cursor) = next_cursor {
        if let Some(status_filter) = status_filter {
            println!(
                "More runs are available. Continue with --status {status_filter} --after {next_cursor}"
            );
        } else {
            println!("More runs are available. Continue with --after {next_cursor}");
        }
    }
    Ok(())
}
