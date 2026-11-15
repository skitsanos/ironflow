use std::collections::HashMap;
use std::process::Stdio;
use std::time::Duration;

use anyhow::{Result, anyhow, bail};
use http::{HeaderName, HeaderValue};
use rmcp::model::ClientConfig;
use rmcp::transport::StreamableHttpClientTransport;
use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;
use serde_json::Value;
use tokio::process::Command;

use crate::nodes::child_process::{ChildProcessGuard, configure_command};
use crate::util::outbound::{Guard, ensure_url_allowed};

use super::config::McpTransport;
use super::session::McpSession;
use super::stdio::StrictStdioTransport;

pub(super) async fn initialize(
    config: &Value,
    client_info: ClientConfig,
    transport: McpTransport,
    timeout: Duration,
) -> Result<McpSession> {
    match transport {
        McpTransport::Stdio => initialize_stdio(config, client_info, timeout).await,
        McpTransport::StreamableHttp => {
            initialize_streamable_http(config, client_info, timeout).await
        }
    }
}

async fn initialize_stdio(
    config: &Value,
    client_info: ClientConfig,
    timeout: Duration,
) -> Result<McpSession> {
    let executable = config
        .get("command")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("mcp_client stdio requires 'command'"))?;
    let mut command = Command::new(executable);

    if let Some(args) = config.get("args").and_then(Value::as_array) {
        for argument in args {
            command.arg(argument.as_str().ok_or_else(|| {
                anyhow!("mcp_client stdio expects every 'args' entry to be a string")
            })?);
        }
    } else if config.get("args").is_some() {
        bail!("mcp_client stdio expects 'args' to be an array");
    }
    if let Some(cwd) = config.get("cwd").and_then(Value::as_str) {
        command.current_dir(cwd);
    } else if config.get("cwd").is_some() {
        bail!("mcp_client stdio expects 'cwd' to be a string");
    }
    if let Some(environment) = config.get("env").and_then(Value::as_object) {
        for (name, value) in environment {
            command.env(
                name,
                value.as_str().ok_or_else(|| {
                    anyhow!("mcp_client stdio expects every 'env' value to be a string")
                })?,
            );
        }
    } else if config.get("env").is_some() {
        bail!("mcp_client stdio expects 'env' to be an object");
    }

    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    configure_command(&mut command);
    let child = command
        .spawn()
        .map_err(|error| anyhow!("mcp_client: failed to start '{executable}': {error}"))?;
    let process_guard = ChildProcessGuard::new(&child);
    let transport = StrictStdioTransport::new(child, process_guard.clone())?;
    let service = tokio::time::timeout(timeout, rmcp::serve_client(client_info, transport))
        .await
        .map_err(|_| {
            anyhow!(
                "mcp_client: stdio initialization timed out after {}s",
                timeout.as_secs_f64()
            )
        })?
        .map_err(|error| anyhow!("mcp_client: stdio initialization failed: {error}"))?;

    Ok(McpSession::new(
        service,
        McpTransport::Stdio,
        Some(process_guard),
    ))
}

async fn initialize_streamable_http(
    config: &Value,
    client_info: ClientConfig,
    timeout: Duration,
) -> Result<McpSession> {
    let url = config
        .get("url")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("mcp_client streamable_http requires 'url'"))?;
    let transport_config = StreamableHttpClientTransportConfig::with_uri(url.to_string())
        .custom_headers(custom_headers(config)?);
    // Without the guard the transport keeps its own default client.
    let transport = match Guard::resolve(false)? {
        Some(guard) => {
            ensure_url_allowed("mcp_client", url).await?;
            StreamableHttpClientTransport::with_client(guarded_client(guard)?, transport_config)
        }
        None => StreamableHttpClientTransport::from_config(transport_config),
    };
    let service = tokio::time::timeout(timeout, rmcp::serve_client(client_info, transport))
        .await
        .map_err(|_| {
            anyhow!(
                "mcp_client: Streamable HTTP initialization timed out after {}s",
                timeout.as_secs_f64()
            )
        })?
        .map_err(|error| anyhow!("mcp_client: Streamable HTTP initialization failed: {error}"))?;

    Ok(McpSession::new(service, McpTransport::StreamableHttp, None))
}

/// The transport's default client with the outbound destination guard
/// installed (IF-212): direct connections, validated and pinned DNS answers,
/// and the same redirect rule, which follows only same-origin `307` and `308`
/// hops and so never leaves the checked host.
fn guarded_client(guard: Guard) -> Result<reqwest::Client> {
    const MAX_REDIRECTS: usize = 10;

    let redirect_guard = guard.clone();
    let redirects = reqwest::redirect::Policy::custom(move |attempt| {
        let preserves_body = matches!(
            attempt.status(),
            reqwest::StatusCode::TEMPORARY_REDIRECT | reqwest::StatusCode::PERMANENT_REDIRECT
        );
        let same_origin = attempt
            .previous()
            .first()
            .is_some_and(|original| original.origin() == attempt.url().origin());
        if !preserves_body || !same_origin {
            attempt.stop()
        } else if attempt.previous().len() > MAX_REDIRECTS {
            attempt.error(format!("exceeded {MAX_REDIRECTS} redirects"))
        } else if let Err(refusal) = redirect_guard.check_url(attempt.url()) {
            attempt.error(refusal)
        } else {
            attempt.follow()
        }
    });
    guard
        .install(
            reqwest::Client::builder()
                .pool_max_idle_per_host(0)
                .redirect(redirects),
        )
        .build()
        .map_err(|_| anyhow!("mcp_client: failed to build the Streamable HTTP client"))
}

fn custom_headers(config: &Value) -> Result<HashMap<HeaderName, HeaderValue>> {
    let Some(headers) = config.get("headers") else {
        return Ok(HashMap::new());
    };
    let headers = headers
        .as_object()
        .ok_or_else(|| anyhow!("mcp_client streamable_http expects 'headers' to be an object"))?;
    let mut parsed = HashMap::with_capacity(headers.len());
    for (name, value) in headers {
        let normalized = name.to_ascii_lowercase();
        if matches!(
            normalized.as_str(),
            "accept" | "content-type" | "mcp-session-id" | "mcp-protocol-version"
        ) {
            bail!("mcp_client: header '{name}' is transport-managed and cannot be configured");
        }
        let name = HeaderName::from_bytes(name.as_bytes())
            .map_err(|error| anyhow!("mcp_client: invalid HTTP header name: {error}"))?;
        let value = value
            .as_str()
            .ok_or_else(|| anyhow!("mcp_client HTTP header values must be strings"))?;
        let value = HeaderValue::from_str(value)
            .map_err(|error| anyhow!("mcp_client: invalid HTTP header value: {error}"))?;
        parsed.insert(name, value);
    }
    Ok(parsed)
}
