use std::future::Future;
use std::sync::Arc;

use crate::engine::types::Context;
use crate::util::redaction::SecretRedactor;

tokio::task_local! {
    static CURRENT_EXECUTION_OVERLAY: ExecutionOverlay;
}

/// Invocation-only values and their persistence redaction policy.
///
/// The values are merged into every node input, but never into the durable
/// initial or final context. The task-local copy lets composition nodes carry
/// the same policy into child workflow engines without widening the Node API.
#[derive(Clone, Debug)]
pub(crate) struct ExecutionOverlay {
    values: Arc<Context>,
    redactor: SecretRedactor,
}

impl Default for ExecutionOverlay {
    fn default() -> Self {
        Self::new(Context::new())
    }
}

impl ExecutionOverlay {
    pub(crate) fn new(values: Context) -> Self {
        let redactor = SecretRedactor::from_overlay(&values);
        Self {
            values: Arc::new(values),
            redactor,
        }
    }

    pub(crate) fn current() -> Self {
        CURRENT_EXECUTION_OVERLAY
            .try_with(Clone::clone)
            .unwrap_or_default()
    }

    pub(crate) fn values(&self) -> &Context {
        self.values.as_ref()
    }

    pub(crate) fn redact_context(&self, context: &Context) -> Context {
        self.redactor.redact_context(context)
    }

    pub(crate) fn redact_text(&self, text: &str) -> String {
        self.redactor.redact_text(text)
    }

    pub(crate) fn strip_from_context(&self, context: &mut Context) {
        for key in self.values.keys() {
            context.remove(key);
        }
    }

    pub(crate) async fn scope<F>(&self, future: F) -> F::Output
    where
        F: Future,
    {
        CURRENT_EXECUTION_OVERLAY.scope(self.clone(), future).await
    }
}
