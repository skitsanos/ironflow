use anyhow::{Result, anyhow, bail};
use serde_json::{Map, Value, json};
use std::sync::atomic::{AtomicI64, Ordering};

use crate::engine::types::Context;
use crate::lua::interpolate::interpolate_ctx;

pub(super) const DEFAULT_TIMEOUT_SECONDS: f64 = 30.0;
const DEFAULT_CLIENT_NAME: &str = "ironflow";
const DEFAULT_CLIENT_VERSION: &str = env!("CARGO_PKG_VERSION");
pub(super) const DEFAULT_PROTOCOL_VERSION: &str = "2024-11-05";

static REQUEST_ID: AtomicI64 = AtomicI64::new(1);

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum McpTransport {
    Stdio,
    Sse,
}

#[derive(Clone, Copy, PartialEq)]
pub(super) enum McpAction {
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

pub(super) fn next_request_id() -> i64 {
    REQUEST_ID.fetch_add(1, Ordering::SeqCst)
}

pub(super) fn interpolate_json_value(value: &Value, ctx: &Context) -> Value {
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

pub(super) fn transport_from_config(config: &Value) -> Result<McpTransport> {
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

pub(super) fn action_from_config(config: &Value) -> Result<McpAction> {
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

pub(super) fn output_key(config: &Value) -> &str {
    config
        .get("output_key")
        .and_then(Value::as_str)
        .unwrap_or("mcp")
}

pub(super) fn timeout_seconds(config: &Value) -> f64 {
    config
        .get("timeout")
        .and_then(Value::as_f64)
        .unwrap_or(DEFAULT_TIMEOUT_SECONDS)
}

pub(super) fn request_id(config: &Value, ctx: &Context) -> Value {
    config
        .get("request_id")
        .map(|value| interpolate_json_value(value, ctx))
        .filter(|value| matches!(value, Value::String(_) | Value::Number(_)))
        .unwrap_or_else(|| Value::from(next_request_id()))
}

pub(super) fn method_for_action(action: McpAction) -> &'static str {
    match action {
        McpAction::Initialize => "initialize",
        McpAction::Initialized => "notifications/initialized",
        McpAction::ListTools => "tools/list",
        McpAction::CallTool => "tools/call",
    }
}

pub(super) fn resolve_protocol_version(config: &Value) -> String {
    config
        .get("protocol_version")
        .and_then(Value::as_str)
        .unwrap_or(DEFAULT_PROTOCOL_VERSION)
        .to_string()
}

pub(super) fn is_sse_auto_initialize_enabled(config: &Value) -> Result<bool> {
    if let Some(value) = config.get("auto_initialize") {
        value
            .as_bool()
            .ok_or_else(|| anyhow!("mcp_client auto_initialize must be true or false"))
    } else {
        Ok(true)
    }
}

pub(super) fn normalize_header_name(name: &str) -> &str {
    match name.to_ascii_lowercase().as_str() {
        "mcp-session-id" => "Mcp-Session-Id",
        "mcp-protocol-version" => "Mcp-Protocol-Version",
        _ => name,
    }
}

pub(super) fn build_initialize_request(config: &Value, ctx: &Context) -> Result<Value> {
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

pub(super) fn build_list_tools_request(config: &Value, ctx: &Context) -> Result<Value> {
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

pub(super) fn build_initialized_request(config: &Value, ctx: &Context) -> Result<Value> {
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

pub(super) fn build_call_tool_request(config: &Value, ctx: &Context) -> Result<Value> {
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

pub(super) fn build_request(action: McpAction, config: &Value, ctx: &Context) -> Result<Value> {
    match action {
        McpAction::Initialize => build_initialize_request(config, ctx),
        McpAction::Initialized => build_initialized_request(config, ctx),
        McpAction::ListTools => build_list_tools_request(config, ctx),
        McpAction::CallTool => build_call_tool_request(config, ctx),
    }
}

pub(super) fn check_rpc_response(response: &Value, action: McpAction) -> Result<Value> {
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

pub(super) fn tool_text_from_content(content: &Value) -> Option<String> {
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
