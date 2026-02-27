use anyhow::Result;
use async_trait::async_trait;
use lettre::message::{Mailbox, MultiPart, SinglePart, header::ContentType};
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use std::time::Duration;

use crate::engine::types::{Context, NodeOutput};
use crate::lua::interpolate::interpolate_ctx;
use crate::nodes::Node;

fn interpolate_json_value(value: &serde_json::Value, ctx: &Context) -> serde_json::Value {
    match value {
        serde_json::Value::String(s) => serde_json::Value::String(interpolate_ctx(s, ctx)),
        serde_json::Value::Array(items) => serde_json::Value::Array(
            items
                .iter()
                .map(|item| interpolate_json_value(item, ctx))
                .collect(),
        ),
        serde_json::Value::Object(map) => serde_json::Value::Object(
            map.iter()
                .map(|(key, value)| (key.clone(), interpolate_json_value(value, ctx)))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn resolve_param(
    config: &serde_json::Value,
    key: &str,
    env_key: &str,
    ctx: &Context,
) -> Option<String> {
    config
        .get(key)
        .and_then(|v| v.as_str())
        .map(|v| interpolate_ctx(v, ctx))
        .or_else(|| std::env::var(env_key).ok())
}

fn resolve_output_key(config: &serde_json::Value) -> String {
    config
        .get("output_key")
        .and_then(|v| v.as_str())
        .unwrap_or("email")
        .to_string()
}

/// Normalize `to` field: accepts a single string or an array of strings.
fn resolve_recipients(value: &serde_json::Value, ctx: &Context) -> Option<Vec<String>> {
    match value {
        serde_json::Value::String(s) => {
            let interpolated = interpolate_ctx(s, ctx);
            Some(vec![interpolated])
        }
        serde_json::Value::Array(arr) => {
            let list: Vec<String> = arr
                .iter()
                .filter_map(|v| v.as_str().map(|s| interpolate_ctx(s, ctx)))
                .collect();
            if list.is_empty() { None } else { Some(list) }
        }
        _ => None,
    }
}

pub struct SendEmailNode;

#[async_trait]
impl Node for SendEmailNode {
    fn node_type(&self) -> &str {
        "send_email"
    }

    fn description(&self) -> &str {
        "Send an email via Resend API or SMTP"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
        let provider = config
            .get("provider")
            .and_then(|v| v.as_str())
            .unwrap_or("resend");

        match provider {
            "resend" => self.send_via_resend(config, &ctx).await,
            "smtp" => self.send_via_smtp(config, &ctx).await,
            other => anyhow::bail!("send_email: unsupported provider '{}'", other),
        }
    }
}

struct EmailParams {
    to: Vec<String>,
    from: String,
    subject: String,
    html: Option<String>,
    text: Option<String>,
    cc: Option<Vec<String>>,
    bcc: Option<Vec<String>>,
    reply_to: Option<String>,
    output_key: String,
    timeout: Duration,
}

fn resolve_string_list(value: &serde_json::Value, ctx: &Context) -> Option<Vec<String>> {
    resolve_recipients(value, ctx)
}

fn extract_common_params(config: &serde_json::Value, ctx: &Context) -> Result<EmailParams> {
    let to_value = config
        .get("to")
        .ok_or_else(|| anyhow::anyhow!("send_email requires 'to' field"))?;
    let to = resolve_recipients(to_value, ctx)
        .ok_or_else(|| anyhow::anyhow!("send_email 'to' must be a string or array of strings"))?;

    let subject = config
        .get("subject")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("send_email requires 'subject' field"))?;
    let subject = interpolate_ctx(subject, ctx);

    let from = resolve_param(config, "from", "SENDER_EMAIL", ctx)
        .unwrap_or_else(|| "onboarding@resend.dev".to_string());

    let output_key = resolve_output_key(config);
    let timeout_s = config
        .get("timeout")
        .and_then(|v| v.as_f64())
        .unwrap_or(30.0);
    let timeout = Duration::from_secs_f64(timeout_s);

    let html = config
        .get("html")
        .and_then(|v| v.as_str())
        .map(|v| interpolate_ctx(v, ctx));
    let text = config
        .get("text")
        .and_then(|v| v.as_str())
        .map(|v| interpolate_ctx(v, ctx));

    let cc = config.get("cc").and_then(|v| resolve_string_list(v, ctx));
    let bcc = config.get("bcc").and_then(|v| resolve_string_list(v, ctx));
    let reply_to = config
        .get("reply_to")
        .and_then(|v| v.as_str())
        .map(|v| interpolate_ctx(v, ctx));

    Ok(EmailParams {
        to,
        from,
        subject,
        html,
        text,
        cc,
        bcc,
        reply_to,
        output_key,
        timeout,
    })
}

impl SendEmailNode {
    async fn send_via_resend(
        &self,
        config: &serde_json::Value,
        ctx: &Context,
    ) -> Result<NodeOutput> {
        let api_key = resolve_param(config, "api_key", "RESEND_API_KEY", ctx).ok_or_else(|| {
            anyhow::anyhow!("send_email requires 'api_key' or RESEND_API_KEY env var")
        })?;

        let params = extract_common_params(config, ctx)?;

        // Build Resend API payload
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
        if let Some(cc) = config.get("cc") {
            payload["cc"] = interpolate_json_value(cc, ctx);
        }
        if let Some(bcc) = config.get("bcc") {
            payload["bcc"] = interpolate_json_value(bcc, ctx);
        }
        if let Some(reply_to) = config.get("reply_to") {
            payload["reply_to"] = interpolate_json_value(reply_to, ctx);
        }

        let api_url = config
            .get("api_url")
            .and_then(|v| v.as_str())
            .unwrap_or("https://api.resend.com/emails");

        let user_agent = format!(
            "IronFlow {}, https://github.com/skitsanos/ironflow",
            env!("CARGO_PKG_VERSION")
        );

        let client = reqwest::Client::builder().timeout(params.timeout).build()?;
        let response = client
            .post(api_url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("User-Agent", &user_agent)
            .json(&payload)
            .send()
            .await?;

        let status = response.status().as_u16();
        let success = response.status().is_success();
        let body = response.text().await?;
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
            anyhow::bail!("send_email Resend API returned status {}: {}", status, body);
        }

        Ok(output)
    }

    async fn send_via_smtp(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let params = extract_common_params(config, ctx)?;

        let smtp_server =
            resolve_param(config, "smtp_server", "SMTP_SERVER", ctx).ok_or_else(|| {
                anyhow::anyhow!(
                    "send_email smtp provider requires 'smtp_server' or SMTP_SERVER env var"
                )
            })?;

        let smtp_port = config
            .get("smtp_port")
            .and_then(|v| v.as_u64())
            .map(|v| v as u16)
            .or_else(|| std::env::var("SMTP_PORT").ok().and_then(|v| v.parse().ok()));

        let smtp_username = resolve_param(config, "smtp_username", "SMTP_USERNAME", ctx);
        let smtp_password = resolve_param(config, "smtp_password", "SMTP_PASSWORD", ctx);

        let tls_mode = config
            .get("smtp_tls")
            .and_then(|v| v.as_str())
            .unwrap_or("starttls");

        // Build the email message
        let from_mailbox: Mailbox = params.from.parse().map_err(|e| {
            anyhow::anyhow!(
                "send_email: invalid 'from' address '{}': {}",
                params.from,
                e
            )
        })?;

        let mut builder = Message::builder()
            .from(from_mailbox)
            .subject(&params.subject);

        for addr in &params.to {
            let mailbox: Mailbox = addr.parse().map_err(|e| {
                anyhow::anyhow!("send_email: invalid 'to' address '{}': {}", addr, e)
            })?;
            builder = builder.to(mailbox);
        }

        if let Some(cc_list) = &params.cc {
            for addr in cc_list {
                let mailbox: Mailbox = addr.parse().map_err(|e| {
                    anyhow::anyhow!("send_email: invalid 'cc' address '{}': {}", addr, e)
                })?;
                builder = builder.cc(mailbox);
            }
        }

        if let Some(bcc_list) = &params.bcc {
            for addr in bcc_list {
                let mailbox: Mailbox = addr.parse().map_err(|e| {
                    anyhow::anyhow!("send_email: invalid 'bcc' address '{}': {}", addr, e)
                })?;
                builder = builder.bcc(mailbox);
            }
        }

        if let Some(reply_to) = &params.reply_to {
            let mailbox: Mailbox = reply_to.parse().map_err(|e| {
                anyhow::anyhow!(
                    "send_email: invalid 'reply_to' address '{}': {}",
                    reply_to,
                    e
                )
            })?;
            builder = builder.reply_to(mailbox);
        }

        let email = match (&params.html, &params.text) {
            (Some(html), Some(text)) => builder.multipart(
                MultiPart::alternative()
                    .singlepart(
                        SinglePart::builder()
                            .header(ContentType::TEXT_PLAIN)
                            .body(text.clone()),
                    )
                    .singlepart(
                        SinglePart::builder()
                            .header(ContentType::TEXT_HTML)
                            .body(html.clone()),
                    ),
            )?,
            (Some(html), None) => builder.header(ContentType::TEXT_HTML).body(html.clone())?,
            (None, Some(text)) => builder.header(ContentType::TEXT_PLAIN).body(text.clone())?,
            (None, None) => builder
                .header(ContentType::TEXT_PLAIN)
                .body(String::new())?,
        };

        // Build the SMTP transport
        let transport = match tls_mode {
            "none" => {
                let mut t = AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&smtp_server);
                if let Some(port) = smtp_port {
                    t = t.port(port);
                }
                t = t.timeout(Some(params.timeout));
                if let (Some(user), Some(pass)) = (&smtp_username, &smtp_password) {
                    t = t.credentials(Credentials::new(user.clone(), pass.clone()));
                }
                t.build()
            }
            "tls" => {
                let mut t = AsyncSmtpTransport::<Tokio1Executor>::relay(&smtp_server)?;
                if let Some(port) = smtp_port {
                    t = t.port(port);
                }
                t = t.timeout(Some(params.timeout));
                if let (Some(user), Some(pass)) = (&smtp_username, &smtp_password) {
                    t = t.credentials(Credentials::new(user.clone(), pass.clone()));
                }
                t.build()
            }
            _ => {
                // "starttls" (default)
                let mut t = AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&smtp_server)?;
                if let Some(port) = smtp_port {
                    t = t.port(port);
                }
                t = t.timeout(Some(params.timeout));
                if let (Some(user), Some(pass)) = (&smtp_username, &smtp_password) {
                    t = t.credentials(Credentials::new(user.clone(), pass.clone()));
                }
                t.build()
            }
        };

        let result = transport.send(email).await;

        let mut output = NodeOutput::new();

        match result {
            Ok(response) => {
                let code = response.code().to_string();
                let message: String = format!("{:?}", response);

                output.insert(
                    format!("{}_status", params.output_key),
                    serde_json::Value::String(code.clone()),
                );
                output.insert(
                    format!("{}_data", params.output_key),
                    serde_json::json!({
                        "code": code,
                        "message": message,
                    }),
                );
                output.insert(
                    format!("{}_success", params.output_key),
                    serde_json::Value::Bool(response.is_positive()),
                );

                if !response.is_positive() {
                    anyhow::bail!("send_email SMTP server returned {}: {}", code, message);
                }
            }
            Err(e) => {
                anyhow::bail!("send_email SMTP error: {}", e);
            }
        }

        Ok(output)
    }
}
