use anyhow::Result;
use async_trait::async_trait;
use std::time::Duration;

use crate::engine::types::{Context, NodeOutput};
use crate::lua::interpolate::interpolate_ctx;
use crate::nodes::Node;

fn interpolate_json_value(value: &serde_json::Value, ctx: &Context) -> serde_json::Value {
    match value {
        serde_json::Value::String(s) => serde_json::Value::String(interpolate_ctx(s, ctx)),
        serde_json::Value::Array(items) => serde_json::Value::Array(
            items.iter().map(|item| interpolate_json_value(item, ctx)).collect(),
        ),
        serde_json::Value::Object(map) => serde_json::Value::Object(
            map.iter()
                .map(|(key, value)| (key.clone(), interpolate_json_value(value, ctx)))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn resolve_output_key(config: &serde_json::Value) -> String {
    config
        .get("output_key")
        .and_then(|value| value.as_str())
        .unwrap_or("slack")
        .to_string()
}

fn resolve_webhook_url(config: &serde_json::Value, ctx: &Context) -> Option<String> {
    config
        .get("webhook_url")
        .and_then(|value| value.as_str())
        .map(|value| interpolate_ctx(value, ctx))
        .or_else(|| std::env::var("SLACK_WEBHOOK").ok())
}

pub struct SlackNotificationNode;

#[async_trait]
impl Node for SlackNotificationNode {
    fn node_type(&self) -> &str {
        "slack_notification"
    }

    fn description(&self) -> &str {
        "Send a Slack message through an incoming webhook URL"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
        let webhook_url = resolve_webhook_url(config, &ctx)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "slack_notification requires 'webhook_url' or SLACK_WEBHOOK env var"
                )
            })?;

        let output_key = resolve_output_key(config);
        let timeout_s = config.get("timeout").and_then(|value| value.as_f64()).unwrap_or(30.0);
        let timeout = Duration::from_secs_f64(timeout_s);

        let mut payload = config
            .get("payload")
            .map(|value| interpolate_json_value(value, &ctx))
            .unwrap_or_else(|| serde_json::json!({}));

        let payload_obj = payload.as_object_mut().ok_or_else(|| {
            anyhow::anyhow!("slack_notification requires 'payload' to be an object when provided")
        })?;

        let text = config
            .get("text")
            .and_then(|value| value.as_str())
            .or_else(|| config.get("message").and_then(|value| value.as_str()))
            .or_else(|| {
                payload_obj
                    .get("text")
                    .and_then(|value| value.as_str())
            })
            .map(|value| interpolate_ctx(value, &ctx));

        if let Some(text) = text {
            payload_obj.insert("text".to_string(), serde_json::Value::String(text));
        } else {
            anyhow::bail!(
                "slack_notification requires 'text', 'message', or 'payload.text' to be present"
            );
        }

        let client = reqwest::Client::builder().timeout(timeout).build()?;
        let response = client
            .post(&webhook_url)
            .json(payload_obj)
            .send()
            .await?;

        let status = response.status().as_u16();
        let success = response.status().is_success();
        let body = response.text().await?;
        let data = serde_json::from_str(&body).unwrap_or(serde_json::Value::String(body.clone()));

        let mut output = NodeOutput::new();
        output.insert(
            format!("{}_status", output_key),
            serde_json::Value::Number(status.into()),
        );
        output.insert(format!("{}_data", output_key), data);
        output.insert(
            format!("{}_success", output_key),
            serde_json::Value::Bool(success),
        );

        if !success {
            anyhow::bail!(
                "Slack webhook returned status {} for '{}': {}",
                status,
                webhook_url,
                body
            );
        }

        Ok(output)
    }
}
