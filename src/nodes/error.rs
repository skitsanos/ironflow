use std::fmt;

use crate::engine::types::NodeOutput;

/// A node error that retains structured diagnostics for terminal publication.
///
/// The executor keeps this output private while a step can still retry. If the
/// structured failure is the step's terminal outcome, the redacted output is
/// used for bounded failed-task history, preserved for its recovery handler,
/// and published at its phase barrier.
pub struct NodeFailure {
    message: String,
    output: NodeOutput,
}

impl NodeFailure {
    /// Create a structured node failure.
    pub fn new(message: impl Into<String>, output: NodeOutput) -> Self {
        Self {
            message: message.into(),
            output,
        }
    }

    /// Borrow the structured output attached to this failure.
    pub fn output(&self) -> &NodeOutput {
        &self.output
    }

    /// Move structured output into the executor while retaining the error
    /// message for retry diagnostics.
    pub(crate) fn take_output(&mut self) -> NodeOutput {
        std::mem::take(&mut self.output)
    }

    /// Consume the failure and return its structured output.
    pub fn into_output(self) -> NodeOutput {
        self.output
    }
}

impl fmt::Debug for NodeFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NodeFailure")
            .field("message", &self.message)
            .field("output_fields", &self.output.len())
            .finish()
    }
}

impl fmt::Display for NodeFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for NodeFailure {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_output_does_not_disclose_structured_values() {
        let failure = NodeFailure::new(
            "provider failed",
            NodeOutput::from([("stderr".to_string(), serde_json::json!("secret diagnostic"))]),
        );

        let debug = format!("{failure:?}");
        assert!(debug.contains("provider failed"));
        assert!(debug.contains("output_fields"));
        assert!(!debug.contains("stderr"));
        assert!(!debug.contains("secret diagnostic"));
    }

    #[test]
    fn anyhow_context_preserves_the_typed_failure() {
        let failure = anyhow::Error::new(NodeFailure::new("failed", NodeOutput::new()))
            .context("outer context");

        assert!(failure.downcast_ref::<NodeFailure>().is_some());
    }
}
