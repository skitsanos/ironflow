use anyhow::Result;

use crate::engine::types::{Context, NodeOutput};
use crate::lua::interpolate::interpolate_value;
use crate::util::sensitive_url::{SecretEndpoint, redact_sensitive_text};

use super::params::{extract, resolve_param};

pub(super) async fn send(config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
    let api_key = resolve_param(config, "api_key", "RESEND_API_KEY", ctx).ok_or_else(|| {
        anyhow::anyhow!("send_email requires 'api_key' or RESEND_API_KEY env var")
    })?;
    let params = extract(config, ctx)?;
    let payload = build_payload(config, ctx, &params);
    let api_url = config
        .get("api_url")
        .and_then(|value| value.as_str())
        .unwrap_or("https://api.resend.com/emails");

    let client = reqwest::Client::builder()
        .timeout(params.timeout)
        .build()
        .map_err(|error| {
            anyhow::anyhow!(
                "Failed to build Resend client: {}",
                redact_sensitive_text(&error.to_string())
            )
        })?;
    let response = client
        .post(api_url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header(
            "User-Agent",
            format!(
                "IronFlow {}, https://github.com/skitsanos/ironflow",
                env!("CARGO_PKG_VERSION")
            ),
        )
        .json(&payload)
        .send()
        .await
        .map_err(|error| {
            anyhow::anyhow!(
                "Resend request to {} failed: {}",
                SecretEndpoint::new(api_url),
                redact_sensitive_text(&error.to_string())
            )
        })?;

    let status = response.status().as_u16();
    let success = response.status().is_success();
    let body = response.text().await.map_err(|error| {
        anyhow::anyhow!(
            "Failed to read Resend response from {}: {}",
            SecretEndpoint::new(api_url),
            redact_sensitive_text(&error.to_string())
        )
    })?;
    let data = serde_json::from_str(&body).unwrap_or(serde_json::Value::String(body.clone()));

    let mut output = NodeOutput::new();
    output.insert(
        format!("{}_status", params.output_key),
        serde_json::Value::Number(status.into()),
    );
    output.insert(format!("{}_data", params.output_key), data);
    output.insert(
        format!("{}_success", params.output_key),
        serde_json::Value::Bool(success),
    );

    if !success {
        anyhow::bail!(
            "send_email Resend API at {} returned status {}: {}",
            SecretEndpoint::new(api_url),
            status,
            redact_sensitive_text(&body)
        );
    }
    Ok(output)
}

fn build_payload(
    config: &serde_json::Value,
    ctx: &Context,
    params: &super::params::EmailParams,
) -> serde_json::Value {
    let mut payload = serde_json::json!({
        "from": params.from,
        "to": params.to,
        "subject": params.subject,
    });
    if let Some(html) = &params.html {
        payload["html"] = serde_json::Value::String(html.clone());
    }
    if let Some(text) = &params.text {
        payload["text"] = serde_json::Value::String(text.clone());
    }
    for key in ["cc", "bcc", "reply_to"] {
        if let Some(value) = config.get(key) {
            payload[key] = interpolate_value(value, ctx);
        }
    }
    payload
}
