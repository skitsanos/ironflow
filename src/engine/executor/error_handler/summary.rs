use super::ExecutionState;
use crate::engine::executor::ExecutionOverlay;
use crate::util::sensitive_url::redact_sensitive_text;

const MAX_FAILURES: usize = 8;
const MAX_TASK_BYTES: usize = 128;
const MAX_MESSAGE_BYTES: usize = 768;
const TRUNCATED: &str = "...[truncated]";

impl ExecutionState {
    /// Only unresolved failures cross the live child boundary. BTreeMap order
    /// keeps selection deterministic without allocating a second task index.
    pub(crate) fn failure_summary(&self, overlay: &ExecutionOverlay) -> Option<String> {
        if self.failures.is_empty() {
            return None;
        }
        let mut entries = Vec::with_capacity(MAX_FAILURES + 1);
        for (name, failure) in self.failures.iter().take(MAX_FAILURES) {
            let name = bounded_redacted(name, MAX_TASK_BYTES, overlay);
            let message = bounded_redacted(&failure.message, MAX_MESSAGE_BYTES, overlay);
            entries.push(format!("task '{name}': {message}"));
        }
        let omitted = self.failures.len().saturating_sub(MAX_FAILURES);
        if omitted > 0 {
            entries.push(format!("{omitted} additional task failure(s) omitted"));
        }
        Some(entries.join("; "))
    }
}

fn bounded_redacted(text: &str, limit: usize, overlay: &ExecutionOverlay) -> String {
    // Redact whole values before truncation to avoid leaking a secret prefix.
    let mut text = redact_sensitive_text(&overlay.redact_text(text));
    if text.len() > limit {
        let mut end = limit - TRUNCATED.len();
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
        text.push_str(TRUNCATED);
    }
    text
}

#[cfg(test)]
mod tests;
