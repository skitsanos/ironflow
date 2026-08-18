use crate::engine::types::Context;

use super::config::ScheduleConfig;

/// Context key naming the schedule that triggered a run.
///
/// Underscore-prefixed, so it stays private when a child result is namespaced
/// (IF-057).
pub const SCHEDULE_CONTEXT_KEY: &str = "_schedule";

pub(super) fn initial_context(
    schedule_name: &str,
    instant_key: &str,
    schedule: &ScheduleConfig,
    path: &str,
) -> Context {
    let mut ctx = schedule.context().clone();
    ctx.insert(
        SCHEDULE_CONTEXT_KEY.to_string(),
        serde_json::Value::String(schedule_name.to_string()),
    );
    ctx.insert(
        super::identity::SCHEDULE_INSTANT_CONTEXT_KEY.to_string(),
        serde_json::Value::String(instant_key.to_string()),
    );
    if let Some(dir) = std::path::Path::new(path).parent() {
        ctx.insert(
            "_flow_dir".to_string(),
            serde_json::Value::String(dir.to_string_lossy().to_string()),
        );
    }
    ctx
}
