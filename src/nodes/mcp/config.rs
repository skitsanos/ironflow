use std::time::Duration;

use anyhow::{Result, anyhow, bail};
use rmcp::model::{ClientCapabilities, ClientInfo, Implementation, ProtocolVersion};
use serde_json::{Map, Value};

use crate::engine::types::Context;
use crate::lua::interpolate::interpolate_value;
use crate::util::duration::positive_duration;

const DEFAULT_TIMEOUT_SECONDS: f64 = 30.0;
const DEFAULT_CLIENT_NAME: &str = "ironflow";
const DEFAULT_CLIENT_VERSION: &str = env!("CARGO_PKG_VERSION");
pub(super) const PROTOCOL_VERSION: &str = "2025-11-25";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum McpTransport {
    Stdio,
    StreamableHttp,
}

impl std::fmt::Display for McpTransport {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Stdio => "stdio",
            Self::StreamableHttp => "streamable_http",
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum McpAction {
    Initialize,
    ListTools,
    CallTool,
    Close,
}

impl std::fmt::Display for McpAction {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Initialize => "initialize",
            Self::ListTools => "list_tools",
            Self::CallTool => "call_tool",
            Self::Close => "close",
        })
    }
}

pub(super) fn interpolate_config(value: &Value, context: &Context) -> Value {
    interpolate_value(value, context)
}

pub(super) fn action(config: &Value) -> Result<McpAction> {
    let action = config
        .get("action")
        .and_then(Value::as_str)
        .unwrap_or("initialize")
        .to_ascii_lowercase();

    match action.as_str() {
        "initialize" => Ok(McpAction::Initialize),
        "list_tools" => Ok(McpAction::ListTools),
        "call_tool" => Ok(McpAction::CallTool),
        "close" => Ok(McpAction::Close),
        "initialized" => bail!(
            "mcp_client: 'initialized' is no longer a public action; initialize now completes the MCP handshake atomically"
        ),
        _ => bail!(
            "mcp_client: invalid action '{action}', expected initialize/list_tools/call_tool/close"
        ),
    }
}

pub(super) fn transport(config: &Value) -> Result<McpTransport> {
    let transport = config
        .get("transport")
        .and_then(Value::as_str)
        .unwrap_or("stdio")
        .to_ascii_lowercase();

    match transport.as_str() {
        "stdio" => Ok(McpTransport::Stdio),
        "streamable_http" | "http" => Ok(McpTransport::StreamableHttp),
        "sse" => bail!(
            "mcp_client: transport 'sse' was replaced by 'streamable_http' in the stable MCP transport contract"
        ),
        _ => bail!(
            "mcp_client: invalid transport '{transport}', expected 'stdio' or 'streamable_http'"
        ),
    }
}

pub(super) fn timeout(config: &Value) -> Result<Duration> {
    let seconds = config
        .get("timeout")
        .and_then(Value::as_f64)
        .unwrap_or(DEFAULT_TIMEOUT_SECONDS);
    positive_duration(seconds, "mcp_client timeout")
}

pub(super) fn output_key(config: &Value) -> &str {
    config
        .get("output_key")
        .and_then(Value::as_str)
        .unwrap_or("mcp")
}

pub(super) fn session_handle(config: &Value) -> Result<&str> {
    config
        .get("session")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            anyhow!("mcp_client: this action requires the opaque 'session' returned by initialize")
        })
}

pub(super) fn client_info(config: &Value) -> Result<ClientInfo> {
    let params = match config.get("params") {
        Some(Value::Object(params)) => params,
        Some(_) => bail!("mcp_client initialize expects 'params' to be an object"),
        None => &Map::new(),
    };

    let requested_version = config
        .get("protocol_version")
        .and_then(Value::as_str)
        .or_else(|| params.get("protocolVersion").and_then(Value::as_str))
        .unwrap_or(PROTOCOL_VERSION);
    if requested_version != PROTOCOL_VERSION {
        bail!(
            "mcp_client: unsupported protocol version '{requested_version}'; this build supports stable MCP {PROTOCOL_VERSION}"
        );
    }

    let capabilities = params
        .get("capabilities")
        .cloned()
        .map(serde_json::from_value::<ClientCapabilities>)
        .transpose()
        .map_err(|error| anyhow!("mcp_client: invalid client capabilities: {error}"))?
        .unwrap_or_default();

    let mut implementation = params
        .get("clientInfo")
        .cloned()
        .map(serde_json::from_value::<Implementation>)
        .transpose()
        .map_err(|error| anyhow!("mcp_client: invalid clientInfo: {error}"))?
        .unwrap_or_else(|| Implementation::new(DEFAULT_CLIENT_NAME, DEFAULT_CLIENT_VERSION));

    if let Some(name) = config.get("client_name").and_then(Value::as_str) {
        implementation.name = name.to_string();
    }
    if let Some(version) = config.get("client_version").and_then(Value::as_str) {
        implementation.version = version.to_string();
    }

    Ok(ClientInfo::new(capabilities, implementation)
        .with_protocol_version(ProtocolVersion::V_2025_11_25))
}

pub(super) fn tool_call(config: &Value) -> Result<(String, Map<String, Value>)> {
    let params = config.get("params").and_then(Value::as_object);
    let name = config
        .get("tool_name")
        .and_then(Value::as_str)
        .or_else(|| {
            params
                .and_then(|params| params.get("name"))
                .and_then(Value::as_str)
        })
        .filter(|name| !name.is_empty())
        .ok_or_else(|| anyhow!("mcp_client call_tool requires a non-empty 'tool_name'"))?;

    let arguments = config
        .get("arguments")
        .or_else(|| params.and_then(|params| params.get("arguments")))
        .cloned()
        .unwrap_or_else(|| Value::Object(Map::new()));
    let arguments = arguments.as_object().cloned().ok_or_else(|| {
        anyhow!("mcp_client call_tool expects 'arguments' to be an object when set")
    })?;

    Ok((name.to_string(), arguments))
}
