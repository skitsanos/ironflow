use std::time::Duration;

use anyhow::Result;

use crate::engine::types::Context;
use crate::lua::interpolate::interpolate_ctx;
use crate::util::duration::positive_duration;
use crate::util::node_config::config_f64_or;

pub(super) struct EmailParams {
    pub(super) to: Vec<String>,
    pub(super) from: String,
    pub(super) subject: String,
    pub(super) html: Option<String>,
    pub(super) text: Option<String>,
    pub(super) cc: Option<Vec<String>>,
    pub(super) bcc: Option<Vec<String>>,
    pub(super) reply_to: Option<String>,
    pub(super) output_key: String,
    pub(super) timeout: Duration,
}

pub(super) fn extract(config: &serde_json::Value, ctx: &Context) -> Result<EmailParams> {
    let to_value = config
        .get("to")
        .ok_or_else(|| anyhow::anyhow!("send_email requires 'to' field"))?;
    let to = resolve_recipients(to_value, ctx)
        .ok_or_else(|| anyhow::anyhow!("send_email 'to' must be a string or array of strings"))?;
    let subject = config
        .get("subject")
        .and_then(|value| value.as_str())
        .ok_or_else(|| anyhow::anyhow!("send_email requires 'subject' field"))?;
    let timeout_seconds = config_f64_or(config, "timeout", ctx, 30.0)?;

    Ok(EmailParams {
        to,
        from: resolve_param(config, "from", "SENDER_EMAIL", ctx)
            .unwrap_or_else(|| "onboarding@resend.dev".to_string()),
        subject: interpolate_ctx(subject, ctx),
        html: interpolated_string(config, "html", ctx),
        text: interpolated_string(config, "text", ctx),
        cc: config
            .get("cc")
            .and_then(|value| resolve_recipients(value, ctx)),
        bcc: config
            .get("bcc")
            .and_then(|value| resolve_recipients(value, ctx)),
        reply_to: interpolated_string(config, "reply_to", ctx),
        output_key: config
            .get("output_key")
            .and_then(|value| value.as_str())
            .unwrap_or("email")
            .to_string(),
        timeout: positive_duration(timeout_seconds, "send_email timeout")?,
    })
}

pub(super) fn resolve_param(
    config: &serde_json::Value,
    key: &str,
    env_key: &str,
    ctx: &Context,
) -> Option<String> {
    config
        .get(key)
        .and_then(|value| value.as_str())
        .map(|value| interpolate_ctx(value, ctx))
        .or_else(|| std::env::var(env_key).ok())
}

fn interpolated_string(config: &serde_json::Value, key: &str, ctx: &Context) -> Option<String> {
    config
        .get(key)
        .and_then(|value| value.as_str())
        .map(|value| interpolate_ctx(value, ctx))
}

fn resolve_recipients(value: &serde_json::Value, ctx: &Context) -> Option<Vec<String>> {
    match value {
        serde_json::Value::String(value) => Some(vec![interpolate_ctx(value, ctx)]),
        serde_json::Value::Array(values) => {
            let recipients: Vec<_> = values
                .iter()
                .filter_map(|value| value.as_str())
                .map(|value| interpolate_ctx(value, ctx))
                .collect();
            (!recipients.is_empty()).then_some(recipients)
        }
        _ => None,
    }
}
