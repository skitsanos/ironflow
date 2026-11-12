use std::borrow::Cow;
use std::future::Future;
use std::sync::Arc;

use serde_json::Value;

use crate::engine::types::Context;
use crate::util::redaction::SecretRedactor;
use crate::util::sensitive_url::MAX_DIAGNOSTIC_BYTES;

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

    pub(crate) fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Scrub values that later steps or a parent run will read. Strings and
    /// keys are redacted as for a record, but JSON types and the engine's
    /// routing keys are kept, so an overlay value cannot change control flow
    /// or the shape of live data (IF-176). Records get the stricter form
    /// through `redact_context`, `redact_context_owned` or
    /// `redact_record_value`.
    pub(crate) fn redact_live_context(&self, context: Context) -> Context {
        self.redactor.redact_live_context(context)
    }

    /// Whether a key is left out of stored records.
    pub(crate) fn withholds_record_key(&self, key: &str) -> bool {
        self.redactor.withholds_key(key)
    }

    /// Record form of one live value.
    pub(crate) fn redact_record_value(&self, value: &Value) -> Value {
        self.redactor.redact_value(value)
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

    /// [`Self::redact_diagnostic`] for text of any size, from async code.
    ///
    /// Overlay secrets are matched in the whole text before the scrubber
    /// keeps its bounded prefix; cutting first would emit the head of a
    /// secret that straddles the cut. That pass is linear, but megabytes of
    /// it would hold an async worker, so text above the scrubber's limit is
    /// redacted on the blocking pool (IF-177). The work is bounded by the
    /// text it owns and is not cancelled with its waiter.
    pub(crate) async fn redact_diagnostic_async(&self, text: String) -> String {
        let overlay = self.clone();
        inline_or_blocking(text.len(), move || overlay.redact_diagnostic(&text))
            .await
            .unwrap_or_else(|| REDACTION_INCOMPLETE.to_string())
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

/// Stands in for a diagnostic whose redaction did not finish, so the
/// unredacted text is never the fallback.
const REDACTION_INCOMPLETE: &str = "diagnostic withheld: redaction did not complete";

/// Run `work` inline when its input fits the scrubber's limit, otherwise on
/// the blocking pool. `None` when the blocking task panicked or the runtime
/// is shutting down.
async fn inline_or_blocking<T, F>(input_bytes: usize, work: F) -> Option<T>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    if input_bytes <= MAX_DIAGNOSTIC_BYTES {
        return Some(work());
    }
    tokio::task::spawn_blocking(work).await.ok()
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

    #[tokio::test]
    async fn only_an_oversized_diagnostic_leaves_the_async_worker() {
        let worker = std::thread::current().id();
        let current_thread = || std::thread::current().id();

        let inline = inline_or_blocking(MAX_DIAGNOSTIC_BYTES, current_thread).await;
        let blocking = inline_or_blocking(MAX_DIAGNOSTIC_BYTES + 1, current_thread).await;

        assert_eq!(inline, Some(worker));
        assert!(blocking.is_some_and(|thread| thread != worker));
    }

    #[tokio::test]
    async fn a_redaction_that_does_not_finish_yields_no_text() {
        let outcome = inline_or_blocking::<String, _>(MAX_DIAGNOSTIC_BYTES + 1, || {
            std::panic::resume_unwind(Box::new("redaction failed"))
        })
        .await;

        assert_eq!(outcome, None);
    }

    #[tokio::test]
    async fn an_overlay_secret_across_the_scrubber_limit_is_not_emitted_in_part() {
        let secret = "overlay-secret-value-0123456789";
        let overlay = ExecutionOverlay::new(Context::from([(
            "_headers".to_string(),
            serde_json::json!({"authorization": secret}),
        )]));
        // The secret starts ten bytes before the limit and ends after it.
        let padding = "x".repeat(MAX_DIAGNOSTIC_BYTES - 10);

        let redacted = overlay
            .redact_diagnostic_async(format!("{padding}{secret} tail"))
            .await;

        assert_eq!(redacted, format!("{padding}[REDACTED]...[truncated]"));
    }
}
