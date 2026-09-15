use std::borrow::Cow;
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

    pub(crate) fn redact_context<'a>(&self, context: &'a Context) -> Cow<'a, Context> {
        if self.values.is_empty() {
            Cow::Borrowed(context)
        } else {
            Cow::Owned(self.redactor.redact_context(context))
        }
    }

    pub(crate) fn redact_context_owned(&self, context: Context) -> Context {
        self.redactor.redact_context_owned(context)
    }

    pub(crate) fn redact_text(&self, text: &str) -> String {
        self.redactor.redact_text(text)
    }

    /// Redact a diagnostic string that may reach logs, events, persisted task
    /// state, or a parent workflow.
    ///
    /// Overlay secrets are replaced first, then URL spans and DSN-style
    /// credential assignments. The order matters: an overlay secret that is a
    /// whole connection string must be matched intact. Running the URL
    /// scrubber first would strip only the userinfo, after which the overlay
    /// value no longer matches and the host and path would leak. Every
    /// diagnostic sink must use this single order.
    pub(crate) fn redact_diagnostic(&self, text: &str) -> String {
        crate::util::sensitive_url::redact_sensitive_text(&self.redact_text(text))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_overlay_borrows_initial_context() {
        let context = Context::from([("value".to_string(), serde_json::json!(true))]);

        let redacted = ExecutionOverlay::default().redact_context(&context);

        assert!(matches!(&redacted, Cow::Borrowed(_)));
    }

    #[test]
    fn protected_overlay_owns_and_redacts_initial_context() {
        let secret = "overlay-secret-value";
        let overlay = ExecutionOverlay::new(Context::from([(
            "_headers".to_string(),
            serde_json::json!({"authorization": secret}),
        )]));
        let context = Context::from([
            ("_headers".to_string(), serde_json::json!(secret)),
            ("copy".to_string(), serde_json::json!(secret)),
        ]);

        let redacted = overlay.redact_context(&context);

        assert!(matches!(&redacted, Cow::Owned(_)));
        assert!(!redacted.contains_key("_headers"));
        assert_eq!(redacted["copy"], "[REDACTED]");
    }

    #[test]
    fn diagnostic_redaction_matches_whole_connection_string_before_url_scrubbing() {
        let secret = "https://user:hunter2@example.com/path";
        let overlay = ExecutionOverlay::new(Context::from([(
            "_database_url".to_string(),
            serde_json::json!(secret),
        )]));
        let diagnostic = format!("connect failed for {secret} (timeout)");

        let redacted = overlay.redact_diagnostic(&diagnostic);

        assert_eq!(redacted, "connect failed for [REDACTED] (timeout)");
        assert!(!redacted.contains("hunter2"), "{redacted}");
        assert!(!redacted.contains("example.com"), "{redacted}");
        // The reverse order is exactly the leak this method exists to prevent.
        let reversed = overlay.redact_text(&crate::util::sensitive_url::redact_sensitive_text(
            &diagnostic,
        ));
        assert!(reversed.contains("example.com"), "{reversed}");
    }
}
