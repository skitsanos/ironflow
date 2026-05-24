use anyhow::{Result, anyhow, bail};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde_json::{Map, Value};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;

use crate::engine::types::Context;

use super::protocol::{
    McpAction, build_initialized_request, check_rpc_response, normalize_header_name,
    resolve_protocol_version,
};
use super::session::mark_session_initialized;

pub(super) fn parse_plain_json(text: &str) -> Option<Value> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }

    serde_json::from_str::<Value>(text).ok().or_else(|| {
        text.lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .next_back()
    })
}

pub(super) fn parse_sse_response(text: &str) -> Option<Value> {
    let mut acc = String::new();
    let mut last = None;

    for line in text.lines() {
        if let Some(data) = line.strip_prefix("data:") {
            let payload = data.trim();
            if payload.is_empty() || payload.eq_ignore_ascii_case("[done]") {
                continue;
            }
            acc.push_str(payload);
            continue;
        }

        if line.trim().is_empty() && !acc.trim().is_empty() {
            if let Ok(value) = serde_json::from_str::<Value>(acc.trim()) {
                last = Some(value);
            }
            acc.clear();
        }
    }

    if !acc.trim().is_empty()
        && let Ok(value) = serde_json::from_str::<Value>(acc.trim())
    {
        last = Some(value);
    }

    last
}

pub(super) fn parse_transport_response(raw: &str, is_sse: bool) -> Result<Value> {
    if is_sse && let Some(value) = parse_sse_response(raw) {
        return Ok(value);
    }

    parse_plain_json(raw).ok_or_else(|| {
        anyhow!(
            "mcp_client: failed to parse response JSON: {}",
            raw.replace('\n', "\\n")
        )
    })
}

pub(super) async fn execute_stdio(
    config: &Value,
    request_payload: &str,
    timeout_s: f64,
) -> Result<String> {
    let command = config
        .get("command")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("mcp_client stdio requires 'command'"))?;

    let mut command = Command::new(command);
    if let Some(args) = config.get("args").and_then(Value::as_array) {
        for arg in args {
            let arg = arg
                .as_str()
                .ok_or_else(|| anyhow!("mcp_client stdio args must be strings"))?;
            command.arg(arg);
        }
    }
    if let Some(cwd) = config.get("cwd").and_then(Value::as_str) {
        command.current_dir(cwd);
    }
    if let Some(env_map) = config.get("env").and_then(Value::as_object) {
        for (key, value) in env_map {
            let value = value
                .as_str()
                .ok_or_else(|| anyhow!("mcp_client stdio env values must be strings"))?;
            command.env(key, value);
        }
    }

    command.stdin(std::process::Stdio::piped());
    command.stdout(std::process::Stdio::piped());
    command.stderr(std::process::Stdio::piped());

    let mut child = command.spawn()?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| anyhow!("mcp_client: cannot open stdio stdin"))?;
    stdin
        .write_all(format!("{request_payload}\n").as_bytes())
        .await?;

    let mut stdout = child.stdout.take().ok_or_else(|| {
        anyhow!("mcp_client stdio requires stdout to be piped when waiting on command output")
    })?;
    let mut stderr = child.stderr.take().ok_or_else(|| {
        anyhow!("mcp_client stdio requires stderr to be piped when waiting on command output")
    })?;

    // Bounded reads so a runaway MCP child process can't push unbounded
    // bytes into the parent's RSS. Cap is reused from the shell limit.
    let max_bytes = crate::util::limits::max_shell_output_bytes() as usize;
    let stdout_handle = tokio::spawn(async move {
        let mut data = Vec::new();
        let mut tmp = [0u8; 8192];
        loop {
            match stdout.read(&mut tmp).await {
                Ok(0) => break,
                Ok(n) => {
                    let remaining = max_bytes.saturating_sub(data.len());
                    if n > remaining {
                        data.extend_from_slice(&tmp[..remaining]);
                        let mut sink = [0u8; 8192];
                        while let Ok(k) = stdout.read(&mut sink).await {
                            if k == 0 {
                                break;
                            }
                        }
                        break;
                    }
                    data.extend_from_slice(&tmp[..n]);
                }
                Err(e) => return Err(e),
            }
        }
        Ok::<_, std::io::Error>(data)
    });
    let stderr_handle = tokio::spawn(async move {
        let mut data = Vec::new();
        let mut tmp = [0u8; 8192];
        loop {
            match stderr.read(&mut tmp).await {
                Ok(0) => break,
                Ok(n) => {
                    let remaining = max_bytes.saturating_sub(data.len());
                    if n > remaining {
                        data.extend_from_slice(&tmp[..remaining]);
                        let mut sink = [0u8; 8192];
                        while let Ok(k) = stderr.read(&mut sink).await {
                            if k == 0 {
                                break;
                            }
                        }
                        break;
                    }
                    data.extend_from_slice(&tmp[..n]);
                }
                Err(e) => return Err(e),
            }
        }
        Ok::<_, std::io::Error>(data)
    });

    let status = tokio::select! {
        status = child.wait() => {
            status?
        }
        _ = tokio::time::sleep(Duration::from_secs_f64(timeout_s)) => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            bail!("mcp_client: stdio command timed out after {timeout_s}s");
        }
    };

    let stdout_data = stdout_handle
        .await
        .map_err(|error| anyhow!("mcp_client: failed to collect stdio stdout: {error}"))?
        .map_err(|error| anyhow!("mcp_client: failed to read stdio stdout: {error}"))?;
    let stderr_data = stderr_handle
        .await
        .map_err(|error| anyhow!("mcp_client: failed to collect stdio stderr: {error}"))?
        .map_err(|error| anyhow!("mcp_client: failed to read stdio stderr: {error}"))?;

    if !status.success() {
        let code = status.code().unwrap_or(-1);
        let stderr = String::from_utf8_lossy(&stderr_data);
        bail!("mcp_client: stdio command exited with status {code}. stderr: {stderr}");
    }

    String::from_utf8(stdout_data).map_err(Into::into)
}

pub(super) async fn execute_sse(
    url: &str,
    headers: &HeaderMap,
    request_payload: &str,
    timeout_s: f64,
) -> Result<(String, Option<String>)> {
    post_sse(url, headers, request_payload, timeout_s).await
}

pub(super) fn prepare_sse_headers(
    config: &Value,
    force_protocol: bool,
) -> Result<(HeaderMap, Option<String>)> {
    let mut headers = HeaderMap::new();
    let mut has_protocol_header = false;
    let mut session_id = None;

    if let Some(header_map) = config.get("headers").and_then(Value::as_object) {
        for (name, value) in header_map {
            let value = value
                .as_str()
                .ok_or_else(|| anyhow!("mcp_client sse header values must be strings"))?;
            let normalized_name = normalize_header_name(name);
            if normalized_name.eq_ignore_ascii_case("mcp-protocol-version") {
                has_protocol_header = true;
            }
            if normalized_name.eq_ignore_ascii_case("mcp-session-id") {
                session_id = Some(value.to_string());
            }
            headers.insert(
                HeaderName::from_bytes(normalized_name.as_bytes())?,
                HeaderValue::from_str(value)?,
            );
        }
    }

    let accept_missing = !headers
        .keys()
        .any(|key| key.as_str().eq_ignore_ascii_case("accept"));
    if accept_missing {
        headers.insert(
            HeaderName::from_static("accept"),
            HeaderValue::from_static("application/json, text/event-stream"),
        );
    }
    if force_protocol && !has_protocol_header {
        headers.insert(
            HeaderName::from_bytes("Mcp-Protocol-Version".as_bytes())?,
            HeaderValue::from_str(&resolve_protocol_version(config))?,
        );
    }

    Ok((headers, session_id))
}

pub(super) async fn post_sse(
    url: &str,
    headers: &HeaderMap,
    request_payload: &str,
    timeout_s: f64,
) -> Result<(String, Option<String>)> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs_f64(timeout_s))
        .build()?;
    let response = client
        .post(url)
        .headers(headers.clone())
        .header("Content-Type", "application/json")
        .body(request_payload.to_string())
        .send()
        .await?;

    if !response.status().is_success() {
        bail!(
            "mcp_client: SSE request failed with status {}",
            response.status()
        );
    }

    let mcp_session_id = response
        .headers()
        .get("mcp-session-id")
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_string());

    // Stream-bound the response body so a misbehaving server can't force
    // unbounded allocation. The cap is the shared HTTP body limit.
    let max_body = crate::util::limits::max_http_body_bytes();
    let mut response = response;
    let mut buf: Vec<u8> = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        if buf.len() as u64 + chunk.len() as u64 > max_body {
            bail!(
                "mcp_client: SSE response exceeds {} bytes (set IRONFLOW_MAX_HTTP_BODY_BYTES to raise)",
                max_body
            );
        }
        buf.extend_from_slice(&chunk);
    }
    let response_text = String::from_utf8(buf)
        .map_err(|e| anyhow!("mcp_client: SSE response is not valid UTF-8: {e}"))?;
    Ok((response_text, mcp_session_id))
}

fn parse_sse_response_text(raw: &str) -> Result<Value> {
    if raw.trim().is_empty() {
        return Ok(Value::Object(Map::new()));
    }

    parse_transport_response(raw, true)
}

pub(super) async fn ensure_initialized_sse(
    config: &Value,
    ctx: &Context,
    url: &str,
    headers: &HeaderMap,
    session_id: Option<&str>,
    timeout_s: f64,
) -> Result<()> {
    let request = build_initialized_request(config, ctx)?;
    let request_payload = serde_json::to_string(&request).map_err(anyhow::Error::msg)?;
    let (raw_response, _) = post_sse(url, headers, &request_payload, timeout_s).await?;
    let response = parse_sse_response_text(&raw_response)?;
    let result = check_rpc_response(&response, McpAction::Initialized)?;
    if result.is_object()
        && let Some(session_id) = session_id
    {
        mark_session_initialized(url, session_id);
    }
    Ok(())
}
