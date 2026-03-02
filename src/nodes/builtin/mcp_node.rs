use anyhow::{Result, anyhow, bail};
use async_trait::async_trait;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde_json::{Map, Value, json};
use std::collections::HashSet;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;

use crate::engine::types::{Context, NodeOutput};
use crate::lua::interpolate::interpolate_ctx;
use crate::nodes::Node;

const DEFAULT_TIMEOUT_SECONDS: f64 = 30.0;
const DEFAULT_CLIENT_NAME: &str = "ironflow";
const DEFAULT_CLIENT_VERSION: &str = env!("CARGO_PKG_VERSION");
const DEFAULT_PROTOCOL_VERSION: &str = "2024-11-05";

static REQUEST_ID: AtomicI64 = AtomicI64::new(1);
static INITIALIZED_SESSIONS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

fn initialized_sessions() -> &'static Mutex<HashSet<String>> {
    INITIALIZED_SESSIONS.get_or_init(|| Mutex::new(HashSet::new()))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum McpTransport {
    Stdio,
    Sse,
}

#[derive(Clone, Copy, PartialEq)]
enum McpAction {
    Initialize,
    Initialized,
    ListTools,
    CallTool,
}

impl std::fmt::Display for McpTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let value = match self {
            McpTransport::Stdio => "stdio",
            McpTransport::Sse => "sse",
        };
        write!(f, "{value}")
    }
}

impl std::fmt::Display for McpAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let value = match self {
            McpAction::Initialize => "initialize",
            McpAction::Initialized => "initialized",
            McpAction::ListTools => "list_tools",
            McpAction::CallTool => "call_tool",
        };
        write!(f, "{value}")
    }
}

fn next_request_id() -> i64 {
    REQUEST_ID.fetch_add(1, Ordering::SeqCst)
}

fn interpolate_json_value(value: &Value, ctx: &Context) -> Value {
    match value {
        Value::String(s) => Value::String(interpolate_ctx(s, ctx)),
        Value::Array(values) => Value::Array(
            values
                .iter()
                .map(|value| interpolate_json_value(value, ctx))
                .collect(),
        ),
        Value::Object(map) => {
            let mapped = map
                .iter()
                .map(|(key, value)| (key.clone(), interpolate_json_value(value, ctx)))
                .collect();
            Value::Object(mapped)
        }
        value => value.clone(),
    }
}

fn transport_from_config(config: &Value) -> Result<McpTransport> {
    let transport = config
        .get("transport")
        .and_then(Value::as_str)
        .unwrap_or("stdio")
        .to_lowercase();

    match transport.as_str() {
        "stdio" => Ok(McpTransport::Stdio),
        "sse" => Ok(McpTransport::Sse),
        _ => bail!("mcp_client: invalid transport '{transport}', expected 'stdio' or 'sse'"),
    }
}

fn action_from_config(config: &Value) -> Result<McpAction> {
    let action = config
        .get("action")
        .and_then(Value::as_str)
        .unwrap_or("initialize")
        .to_lowercase();

    match action.as_str() {
        "initialize" => Ok(McpAction::Initialize),
        "initialized" => Ok(McpAction::Initialized),
        "list_tools" => Ok(McpAction::ListTools),
        "call_tool" => Ok(McpAction::CallTool),
        _ => {
            bail!(
                "mcp_client: invalid action '{action}', expected initialize/initialized/list_tools/call_tool"
            )
        }
    }
}

fn output_key(config: &Value) -> &str {
    config
        .get("output_key")
        .and_then(Value::as_str)
        .unwrap_or("mcp")
}

fn timeout_seconds(config: &Value) -> f64 {
    config
        .get("timeout")
        .and_then(Value::as_f64)
        .unwrap_or(DEFAULT_TIMEOUT_SECONDS)
}

fn request_id(config: &Value, ctx: &Context) -> Value {
    config
        .get("request_id")
        .map(|value| interpolate_json_value(value, ctx))
        .filter(|value| matches!(value, Value::String(_) | Value::Number(_)))
        .unwrap_or_else(|| Value::from(next_request_id()))
}

fn method_for_action(action: McpAction) -> &'static str {
    match action {
        McpAction::Initialize => "initialize",
        McpAction::Initialized => "notifications/initialized",
        McpAction::ListTools => "tools/list",
        McpAction::CallTool => "tools/call",
    }
}

fn resolve_protocol_version(config: &Value) -> String {
    config
        .get("protocol_version")
        .and_then(Value::as_str)
        .unwrap_or(DEFAULT_PROTOCOL_VERSION)
        .to_string()
}

fn is_sse_auto_initialize_enabled(config: &Value) -> Result<bool> {
    if let Some(value) = config.get("auto_initialize") {
        value
            .as_bool()
            .ok_or_else(|| anyhow!("mcp_client auto_initialize must be true or false"))
    } else {
        Ok(true)
    }
}

fn session_cache_key(url: &str, session_id: &str) -> String {
    format!("{url}::{session_id}")
}

fn is_session_initialized(url: &str, session_id: &str) -> bool {
    let key = session_cache_key(url, session_id);
    initialized_sessions()
        .lock()
        .map(|cache| cache.contains(&key))
        .unwrap_or(false)
}

fn mark_session_initialized(url: &str, session_id: &str) {
    let key = session_cache_key(url, session_id);
    if let Ok(mut cache) = initialized_sessions().lock() {
        let _ = cache.insert(key);
    }
}

fn normalize_header_name(name: &str) -> &str {
    match name.to_ascii_lowercase().as_str() {
        "mcp-session-id" => "Mcp-Session-Id",
        "mcp-protocol-version" => "Mcp-Protocol-Version",
        _ => name,
    }
}

fn build_initialize_request(config: &Value, ctx: &Context) -> Result<Value> {
    let mut params = interpolate_json_value(
        config.get("params").unwrap_or(&Value::Object(Map::new())),
        ctx,
    );
    let params = params.as_object_mut().ok_or_else(|| {
        anyhow!("mcp_client initialize expects params to be an object or omitted")
    })?;

    let protocol_version =
        if let Some(value) = config.get("protocol_version").and_then(Value::as_str) {
            value.to_string()
        } else if let Some(value) = params.get("protocolVersion").and_then(Value::as_str) {
            value.to_string()
        } else {
            DEFAULT_PROTOCOL_VERSION.to_string()
        };

    let client_name = if let Some(value) = config.get("client_name").and_then(Value::as_str) {
        value.to_string()
    } else if let Some(value) = params
        .get("clientInfo")
        .and_then(|info| info.get("name"))
        .and_then(Value::as_str)
    {
        value.to_string()
    } else {
        DEFAULT_CLIENT_NAME.to_string()
    };

    let client_version = if let Some(value) = config.get("client_version").and_then(Value::as_str) {
        value.to_string()
    } else if let Some(value) = params
        .get("clientInfo")
        .and_then(|info| info.get("version"))
        .and_then(Value::as_str)
    {
        value.to_string()
    } else {
        DEFAULT_CLIENT_VERSION.to_string()
    };

    if !params.contains_key("protocolVersion") {
        params.insert(
            "protocolVersion".to_string(),
            Value::String(protocol_version),
        );
    }

    let client_info = params
        .entry("clientInfo".to_string())
        .or_insert(Value::Object(Map::new()));
    let client_info = client_info.as_object_mut().ok_or_else(|| {
        anyhow!("mcp_client initialize expects params.clientInfo to be an object when set")
    })?;
    client_info
        .entry("name".to_string())
        .or_insert(Value::String(client_name));
    client_info
        .entry("version".to_string())
        .or_insert(Value::String(client_version));

    if !params.contains_key("capabilities") {
        params.insert("capabilities".to_string(), Value::Object(Map::new()));
    }

    Ok(json!({
        "jsonrpc": "2.0",
        "id": request_id(config, ctx),
        "method": method_for_action(McpAction::Initialize),
        "params": Value::Object(params.clone()),
    }))
}

fn build_list_tools_request(config: &Value, ctx: &Context) -> Result<Value> {
    let params = interpolate_json_value(
        config.get("params").unwrap_or(&Value::Object(Map::new())),
        ctx,
    );
    let params = params.as_object().ok_or_else(|| {
        anyhow!("mcp_client list_tools expects params to be an object or omitted")
    })?;

    Ok(json!({
        "jsonrpc": "2.0",
        "id": request_id(config, ctx),
        "method": method_for_action(McpAction::ListTools),
        "params": Value::Object(params.clone()),
    }))
}

fn build_initialized_request(config: &Value, ctx: &Context) -> Result<Value> {
    let params = interpolate_json_value(
        config.get("params").unwrap_or(&Value::Object(Map::new())),
        ctx,
    );
    let params = params.as_object().ok_or_else(|| {
        anyhow!("mcp_client initialized expects params to be an object or omitted")
    })?;

    Ok(json!({
        "jsonrpc": "2.0",
        "method": method_for_action(McpAction::Initialized),
        "params": Value::Object(params.clone()),
    }))
}

fn build_call_tool_request(config: &Value, ctx: &Context) -> Result<Value> {
    let mut params = interpolate_json_value(
        config.get("params").unwrap_or(&Value::Object(Map::new())),
        ctx,
    );
    let params = params
        .as_object_mut()
        .ok_or_else(|| anyhow!("mcp_client call_tool expects params to be an object or omitted"))?;

    let configured_name = config
        .get("tool_name")
        .and_then(Value::as_str)
        .map(ToString::to_string);
    let params_name = params
        .get("name")
        .and_then(Value::as_str)
        .map(ToString::to_string);
    let tool_name = configured_name
        .or(params_name)
        .ok_or_else(|| anyhow!("mcp_client call_tool requires 'tool_name'"))?;

    if let Some(arguments) = config.get("arguments") {
        params.insert(
            "arguments".to_string(),
            interpolate_json_value(arguments, ctx),
        );
    }
    params.insert("name".to_string(), Value::String(tool_name));

    if !params.contains_key("arguments") {
        params.insert("arguments".to_string(), Value::Object(Map::new()));
    }

    Ok(json!({
        "jsonrpc": "2.0",
        "id": request_id(config, ctx),
        "method": method_for_action(McpAction::CallTool),
        "params": Value::Object(params.clone()),
    }))
}

fn build_request(action: McpAction, config: &Value, ctx: &Context) -> Result<Value> {
    match action {
        McpAction::Initialize => build_initialize_request(config, ctx),
        McpAction::Initialized => build_initialized_request(config, ctx),
        McpAction::ListTools => build_list_tools_request(config, ctx),
        McpAction::CallTool => build_call_tool_request(config, ctx),
    }
}

fn parse_plain_json(text: &str) -> Option<Value> {
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

fn parse_sse_response(text: &str) -> Option<Value> {
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

fn parse_transport_response(raw: &str, is_sse: bool) -> Result<Value> {
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

fn check_rpc_response(response: &Value, action: McpAction) -> Result<Value> {
    if matches!(action, McpAction::Initialized) {
        return Ok(Value::Object(Map::new()));
    }

    if let Some(error) = response.get("error") {
        let details = error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        bail!("MCP server returned error for {action}: {details}");
    }

    response
        .get("result")
        .cloned()
        .ok_or_else(|| anyhow!("MCP response for {action} must contain a top-level result field"))
}

fn tool_text_from_content(content: &Value) -> Option<String> {
    let mut pieces = Vec::new();

    match content {
        Value::String(text) => pieces.push(text.clone()),
        Value::Array(values) => {
            for value in values {
                if let Some(text) = value.get("text").and_then(Value::as_str) {
                    pieces.push(text.to_string());
                }
            }
        }
        Value::Object(map) => {
            if let Some(text) = map.get("text").and_then(Value::as_str) {
                pieces.push(text.to_string());
            }
        }
        _ => {}
    }

    if pieces.is_empty() {
        None
    } else {
        Some(pieces.join(""))
    }
}

async fn execute_stdio(config: &Value, request_payload: &str, timeout_s: f64) -> Result<String> {
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

    let stdout_handle = tokio::spawn(async move {
        let mut data = Vec::new();
        let result = stdout.read_to_end(&mut data).await;
        result.map(|_| data)
    });
    let stderr_handle = tokio::spawn(async move {
        let mut data = Vec::new();
        let result = stderr.read_to_end(&mut data).await;
        result.map(|_| data)
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

async fn execute_sse(
    url: &str,
    headers: &HeaderMap,
    request_payload: &str,
    timeout_s: f64,
) -> Result<(String, Option<String>)> {
    post_sse(url, headers, request_payload, timeout_s).await
}

fn prepare_sse_headers(
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

async fn post_sse(
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

    let response_text = response.text().await?;
    Ok((response_text, mcp_session_id))
}

fn parse_sse_response_text(raw: &str) -> Result<Value> {
    if raw.trim().is_empty() {
        return Ok(Value::Object(Map::new()));
    }

    parse_transport_response(raw, true)
}

async fn ensure_initialized_sse(
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
struct CommonMcpOutput<'a> {
    transport: McpTransport,
    action: McpAction,
    request_id: &'a Value,
    request: &'a Value,
    response: &'a Value,
    result: &'a Value,
}

fn append_common_output(output: &mut NodeOutput, output_key: &str, values: &CommonMcpOutput<'_>) {
    output.insert(
        format!("{output_key}_transport"),
        Value::String(values.transport.to_string()),
    );
    output.insert(
        format!("{output_key}_action"),
        Value::String(values.action.to_string()),
    );
    output.insert(
        format!("{output_key}_request_id"),
        values.request_id.clone(),
    );
    output.insert(format!("{output_key}_request"), values.request.clone());
    output.insert(format!("{output_key}_response"), values.response.clone());
    output.insert(format!("{output_key}_result"), values.result.clone());
    output.insert(format!("{output_key}_success"), Value::Bool(true));
}

fn append_initialize_output(output: &mut NodeOutput, output_key: &str, result: &Value) {
    output.insert(
        format!("{output_key}_protocol_version"),
        result
            .get("protocolVersion")
            .cloned()
            .unwrap_or(Value::Null),
    );
    output.insert(
        format!("{output_key}_capabilities"),
        result.get("capabilities").cloned().unwrap_or(Value::Null),
    );
    output.insert(
        format!("{output_key}_server_info"),
        result.get("serverInfo").cloned().unwrap_or(Value::Null),
    );
}

fn append_list_tools_output(output: &mut NodeOutput, output_key: &str, result: &Value) {
    let tools = result.get("tools").cloned().unwrap_or(Value::Array(vec![]));
    let tool_names: Vec<Value> = tools
        .as_array()
        .map(|values| {
            values
                .iter()
                .filter_map(|tool| {
                    tool.get("name")
                        .and_then(Value::as_str)
                        .map(|name| Value::String(name.to_string()))
                })
                .collect()
        })
        .unwrap_or_default();

    output.insert(format!("{output_key}_tools"), tools);
    output.insert(
        format!("{output_key}_tool_names"),
        Value::Array(tool_names.clone()),
    );
    output.insert(
        format!("{output_key}_tool_count"),
        Value::Number(serde_json::Number::from(tool_names.len())),
    );
}

fn append_call_tool_output(
    output: &mut NodeOutput,
    output_key: &str,
    result: &Value,
    tool_name: &str,
) {
    let content = result.get("content").cloned().unwrap_or(Value::Null);
    output.insert(
        format!("{output_key}_tool_name"),
        Value::String(tool_name.to_string()),
    );
    output.insert(format!("{output_key}_tool_result"), result.clone());
    output.insert(format!("{output_key}_tool_content"), content.clone());
    output.insert(
        format!("{output_key}_tool_text"),
        tool_text_from_content(&content)
            .map(Value::String)
            .unwrap_or(Value::Null),
    );
}

pub struct McpClientNode;

#[async_trait]
impl Node for McpClientNode {
    fn node_type(&self) -> &str {
        "mcp_client"
    }

    fn description(&self) -> &str {
        "MCP client with stdio and SSE transports"
    }

    async fn execute(&self, config: &Value, ctx: Context) -> Result<NodeOutput> {
        let config = interpolate_json_value(config, &ctx);
        let transport = transport_from_config(&config)?;
        let action = action_from_config(&config)?;
        let output_key = output_key(&config).to_string();
        let timeout_s = timeout_seconds(&config);

        let request = build_request(action, &config, &ctx)?;
        let request_id = request
            .get("id")
            .cloned()
            .unwrap_or_else(|| Value::Number(next_request_id().into()));
        let request_payload = serde_json::to_string(&request).map_err(anyhow::Error::msg)?;

        let (raw_response, session_id) = match transport {
            McpTransport::Stdio => (
                execute_stdio(&config, &request_payload, timeout_s).await?,
                None,
            ),
            McpTransport::Sse => {
                let url = config
                    .get("url")
                    .and_then(Value::as_str)
                    .ok_or_else(|| anyhow!("mcp_client sse requires 'url'"))?;

                let force_protocol = action != McpAction::Initialize;
                let (headers, session_id) = prepare_sse_headers(&config, force_protocol)?;

                let auto_initialize = is_sse_auto_initialize_enabled(&config)?;
                if auto_initialize
                    && !matches!(action, McpAction::Initialize | McpAction::Initialized)
                    && let Some(session_id) = session_id.as_deref()
                    && !is_session_initialized(url, session_id)
                {
                    ensure_initialized_sse(
                        &config,
                        &ctx,
                        url,
                        &headers,
                        Some(session_id),
                        timeout_s,
                    )
                    .await?;
                }

                execute_sse(url, &headers, &request_payload, timeout_s).await?
            }
        };

        let treat_as_sse = matches!(transport, McpTransport::Sse);
        let response = if raw_response.trim().is_empty() {
            Value::Object(Map::new())
        } else {
            parse_transport_response(&raw_response, treat_as_sse)?
        };
        let result = check_rpc_response(&response, action)?;

        let mut output = NodeOutput::new();
        append_common_output(
            &mut output,
            &output_key,
            &CommonMcpOutput {
                transport,
                action,
                request_id: &request_id,
                request: &request,
                response: &response,
                result: &result,
            },
        );
        if let Some(session_id) = session_id {
            output.insert(
                format!("{output_key}_session_id"),
                Value::String(session_id),
            );
        }

        match action {
            McpAction::Initialized => {}
            McpAction::Initialize => append_initialize_output(&mut output, &output_key, &result),
            McpAction::ListTools => append_list_tools_output(&mut output, &output_key, &result),
            McpAction::CallTool => {
                let tool_name = config
                    .get("tool_name")
                    .and_then(Value::as_str)
                    .or_else(|| {
                        request
                            .get("params")
                            .and_then(|params| params.get("name"))
                            .and_then(Value::as_str)
                    })
                    .unwrap_or("unknown");
                append_call_tool_output(&mut output, &output_key, &result, tool_name);
            }
        }

        Ok(output)
    }
}
