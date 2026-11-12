//! Resolve the flow file of every fan-out entry before any child starts.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context as AnyhowContext, Result};
use serde_json::Value;

use super::super::parallel_runner::ChildPlan;
use super::inputs::FlowEntry;

/// Pair each entry with its canonical flow path.
///
/// Resolution happens off the async worker and once per distinct path: a
/// dynamic fan-out names one flow for every item, so it costs one filesystem
/// call instead of one per item (IF-188).
pub(super) async fn plan_children(
    entries: Vec<FlowEntry>,
    flow_dir: Option<&str>,
) -> Result<Vec<ChildPlan>> {
    let mut resolved: HashMap<PathBuf, Arc<str>> = HashMap::new();
    let mut plans = Vec::with_capacity(entries.len());

    for (index, entry) in entries.into_iter().enumerate() {
        let flow_file = entry
            .config
            .get("flow")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "parallel_subworkflows: each flow entry requires a 'flow' field (index {index})"
                )
            })?;
        let flow_path = if PathBuf::from(flow_file).is_absolute() {
            PathBuf::from(flow_file)
        } else {
            let dir = flow_dir.ok_or_else(|| {
                anyhow::anyhow!(
                    "parallel_subworkflows: cannot resolve relative path '{flow_file}' — _flow_dir not set"
                )
            })?;
            PathBuf::from(dir).join(flow_file)
        };

        let canonical = match resolved.get(&flow_path) {
            Some(canonical) => canonical.clone(),
            None => {
                let canonical: Arc<str> = tokio::fs::canonicalize(&flow_path)
                    .await
                    .with_context(|| {
                        format!(
                            "parallel_subworkflows: cannot find '{}'",
                            flow_path.display()
                        )
                    })?
                    .to_string_lossy()
                    .into();
                resolved.insert(flow_path, canonical.clone());
                canonical
            }
        };
        plans.push(ChildPlan {
            index,
            flow_path: canonical,
            entry,
        });
    }
    Ok(plans)
}
