use anyhow::Result;
use async_trait::async_trait;

use crate::engine::types::{Context, NodeOutput};
use crate::nodes::Node;

mod params;
mod resend;
mod smtp;

pub struct SendEmailNode;

#[async_trait]
impl Node for SendEmailNode {
    fn node_type(&self) -> &str {
        "send_email"
    }

    fn description(&self) -> &str {
        "Send an email via Resend API or SMTP"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        match config
            .get("provider")
            .and_then(|value| value.as_str())
            .unwrap_or("resend")
        {
            "resend" => resend::send(config, ctx).await,
            "smtp" => smtp::send(config, ctx).await,
            other => anyhow::bail!("send_email: unsupported provider '{}'", other),
        }
    }
}
