use anyhow::Result;
use lettre::message::{Mailbox, MultiPart, SinglePart, header::ContentType};
use lettre::transport::smtp::authentication::Credentials;
use lettre::transport::smtp::client::{Tls, TlsParameters};
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

use crate::engine::types::{Context, NodeOutput};
use crate::util::node_config::config_u64_strict;
use crate::util::sensitive_url::{Connection, redact_sensitive_text};

use super::params::{EmailParams, extract, resolve_param};

pub(super) async fn send(config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
    let params = extract(config, ctx)?;
    let smtp_server =
        resolve_param(config, "smtp_server", "SMTP_SERVER", ctx).ok_or_else(|| {
            anyhow::anyhow!(
                "send_email smtp provider requires 'smtp_server' or SMTP_SERVER env var"
            )
        })?;
    let smtp_port = config_u64_strict(config, "smtp_port", ctx)?
        .map(|value| {
            u16::try_from(value)
                .map_err(|_| anyhow::anyhow!("send_email: 'smtp_port' must be at most 65535"))
        })
        .transpose()?
        .or_else(|| std::env::var("SMTP_PORT").ok().and_then(|v| v.parse().ok()));
    let smtp_username = resolve_param(config, "smtp_username", "SMTP_USERNAME", ctx);
    let smtp_password = resolve_param(config, "smtp_password", "SMTP_PASSWORD", ctx);
    let tls_mode = config
        .get("smtp_tls")
        .and_then(|value| value.as_str())
        .unwrap_or("starttls");

    let email = build_message(&params)?;
    // IF-212: when the destination guard applies, the relay name is resolved
    // and classified here and the transport connects to that validated
    // address, so the driver does not resolve the name a second time. TLS
    // still verifies the configured name.
    let port = smtp_port.unwrap_or_else(|| default_port(tls_mode));
    let pinned = crate::util::outbound::resolve_host_allowed("send_email", &smtp_server, port)
        .await?
        .and_then(|addresses| addresses.first().map(std::net::SocketAddr::ip));
    let transport = build_transport(
        &Relay {
            server: &smtp_server,
            pinned,
            port,
            tls_mode,
        },
        smtp_username.zip(smtp_password),
        params.timeout,
    )?;
    let result = transport.send(email).await;
    build_output(result, &params.output_key, &smtp_server)
}

fn build_message(params: &EmailParams) -> Result<Message> {
    let from: Mailbox = params.from.parse().map_err(|error| {
        anyhow::anyhow!(
            "send_email: invalid 'from' address '{}': {}",
            params.from,
            error
        )
    })?;
    let mut builder = Message::builder().from(from).subject(&params.subject);

    for address in &params.to {
        builder = builder.to(parse_mailbox(address, "to")?);
    }
    if let Some(addresses) = &params.cc {
        for address in addresses {
            builder = builder.cc(parse_mailbox(address, "cc")?);
        }
    }
    if let Some(addresses) = &params.bcc {
        for address in addresses {
            builder = builder.bcc(parse_mailbox(address, "bcc")?);
        }
    }
    if let Some(address) = &params.reply_to {
        builder = builder.reply_to(parse_mailbox(address, "reply_to")?);
    }

    match (&params.html, &params.text) {
        (Some(html), Some(text)) => Ok(builder.multipart(
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
        )?),
        (Some(html), None) => Ok(builder.header(ContentType::TEXT_HTML).body(html.clone())?),
        (None, Some(text)) => Ok(builder.header(ContentType::TEXT_PLAIN).body(text.clone())?),
        (None, None) => Ok(builder
            .header(ContentType::TEXT_PLAIN)
            .body(String::new())?),
    }
}

fn parse_mailbox(address: &str, field: &str) -> Result<Mailbox> {
    address.parse().map_err(|error| {
        anyhow::anyhow!(
            "send_email: invalid '{}' address '{}': {}",
            field,
            address,
            error
        )
    })
}

/// Where and how to reach the relay. `pinned` is the address the destination
/// guard validated; without it the transport resolves `server` itself.
struct Relay<'a> {
    server: &'a str,
    pinned: Option<std::net::IpAddr>,
    port: u16,
    tls_mode: &'a str,
}

/// The ports lettre's `builder_dangerous`, `relay` and `starttls_relay` use.
fn default_port(tls_mode: &str) -> u16 {
    match tls_mode {
        "none" => 25,
        "tls" => 465,
        _ => 587,
    }
}

fn build_transport(
    relay: &Relay<'_>,
    credentials: Option<(String, String)>,
    timeout: std::time::Duration,
) -> Result<AsyncSmtpTransport<Tokio1Executor>> {
    let connect_to = relay
        .pinned
        .map_or_else(|| relay.server.to_owned(), |address| address.to_string());
    // The certificate is always verified against the configured server name.
    let tls_parameters = || {
        TlsParameters::new(relay.server.to_owned())
            .map_err(|error| transport_configuration_error(relay.server, error))
    };
    let mut builder = AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(connect_to)
        .port(relay.port)
        .timeout(Some(timeout));
    builder = match relay.tls_mode {
        "none" => builder,
        "tls" => builder.tls(Tls::Wrapper(tls_parameters()?)),
        _ => builder.tls(Tls::Required(tls_parameters()?)),
    };
    if let Some((username, password)) = credentials {
        builder = builder.credentials(Credentials::new(username, password));
    }
    Ok(builder.build())
}

fn transport_configuration_error(
    server: &str,
    error: lettre::transport::smtp::Error,
) -> anyhow::Error {
    anyhow::anyhow!(
        "Failed to configure SMTP relay {}: {}",
        Connection::new(server),
        redact_sensitive_text(&error.to_string())
    )
}

fn build_output(
    result: Result<lettre::transport::smtp::response::Response, lettre::transport::smtp::Error>,
    output_key: &str,
    server: &str,
) -> Result<NodeOutput> {
    let response = result.map_err(|error| {
        anyhow::anyhow!(
            "send_email SMTP error via {}: {}",
            Connection::new(server),
            redact_sensitive_text(&error.to_string())
        )
    })?;
    let code = response.code().to_string();
    let message = format!("{:?}", response);

    let mut output = NodeOutput::new();
    output.insert(
        format!("{}_status", output_key),
        serde_json::Value::String(code.clone()),
    );
    output.insert(
        format!("{}_data", output_key),
        serde_json::json!({ "code": code, "message": message }),
    );
    output.insert(
        format!("{}_success", output_key),
        serde_json::Value::Bool(response.is_positive()),
    );

    if !response.is_positive() {
        anyhow::bail!(
            "send_email SMTP server returned {}: {}",
            code,
            redact_sensitive_text(&message)
        );
    }
    Ok(output)
}
