use serde_json::Value;

use super::{REDACTED, SecretRedactor};
use crate::engine::types::Context;

/// Top-level output keys the engine reads to route steps
/// (`WorkflowEngine::check_route`, the conditional nodes).
const ROUTING_KEY_PREFIXES: [&str; 2] = ["_route_", "_condition_result_"];

/// Where scrubbed data is going.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Boundary {
    /// Read by later steps or a parent run. Values keep their JSON type and
    /// routing keys are never dropped or rewritten, so an invocation-only
    /// value cannot change which steps run (IF-176).
    Live,
    /// Stored or returned to a client. A number equal to a secret is
    /// replaced too, which changes its type in the record only.
    Record,
}

impl SecretRedactor {
    /// Redact an owned context for a record without copying values that can
    /// be retained. The common no-overlay path returns immediately.
    pub(crate) fn redact_context_owned(&self, context: Context) -> Context {
        self.scrub_context(context, Boundary::Record)
    }

    /// Redact output that stays in the running workflow. Strings and keys
    /// are scrubbed as for a record; numbers and routing keys are left for
    /// the record boundary to handle.
    pub(crate) fn redact_live_context(&self, context: Context) -> Context {
        self.scrub_context(context, Boundary::Live)
    }

    fn scrub_context(&self, mut context: Context, boundary: Boundary) -> Context {
        if self.is_empty() {
            return context;
        }

        context.retain(|key, value| {
            if boundary == Boundary::Live && is_routing_key(key) {
                return true;
            }
            if self.withholds_key(key) {
                return false;
            }
            self.scrub_value(value, boundary);
            true
        });
        context
    }

    fn scrub_value(&self, value: &mut Value, boundary: Boundary) {
        match value {
            Value::String(text) if self.tokens.occur_in(text) => {
                *text = self.tokens.redact(std::mem::take(text));
            }
            Value::Array(items) => {
                for item in items {
                    self.scrub_value(item, boundary);
                }
            }
            Value::Object(map) => map.retain(|key, value| {
                if self.withholds_key(key) {
                    return false;
                }
                self.scrub_value(value, boundary);
                true
            }),
            Value::Number(number)
                if boundary == Boundary::Record && self.tokens.matches_number(number) =>
            {
                *value = Value::String(REDACTED.to_string());
            }
            _ => {}
        }
    }
}

fn is_routing_key(key: &str) -> bool {
    ROUTING_KEY_PREFIXES
        .iter()
        .any(|prefix| key.starts_with(prefix))
}
