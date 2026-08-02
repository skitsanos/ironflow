//! Startup-time validation for configured schedules.
//!
//! `ScheduleConfig::new` never receives `flows_dir` — only `serve` does — so
//! it cannot confirm a schedule's flow actually resolves. Left unchecked, a
//! typo in `flow:` starts the process cleanly and is first reported as a
//! `WARN` at the schedule's next due instant, which is exactly the failure
//! mode startup validation exists to prevent.

use std::collections::HashMap;
use std::path::Path;

use anyhow::{Result, bail};

use crate::api::handlers::resolve_flow_path_in;

use super::config::ScheduleConfig;

/// Resolve every schedule's flow path against `flows_dir`, failing the
/// process now rather than at the schedule's first due instant.
///
/// Kept free of any server state beyond `flows_dir` so it can run before
/// anything else starts, and so it can be exercised directly in tests without
/// booting a server.
pub fn validate_schedule_flows(
    schedules: &HashMap<String, ScheduleConfig>,
    flows_dir: Option<&Path>,
) -> Result<()> {
    if schedules.len() > super::config::MAX_SCHEDULES {
        bail!(
            "schedules contains {} entries, exceeding the {}-entry limit",
            schedules.len(),
            super::config::MAX_SCHEDULES
        );
    }
    // Sorted so a multi-schedule misconfiguration always names the same
    // schedule first, regardless of `HashMap` iteration order.
    let mut names: Vec<&String> = schedules.keys().collect();
    names.sort();

    for name in names {
        super::config::validate_schedule_name(name).map_err(anyhow::Error::msg)?;
        let flow = schedules[name].flow();
        if let Err(error) = resolve_flow_path_in(flow, flows_dir) {
            bail!("schedule '{name}': flow '{flow}' could not be resolved: {error:?}");
        }
    }
    Ok(())
}
