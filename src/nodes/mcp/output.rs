use rmcp::model::{CallToolResult, ListToolsResult, ServerPeerInfo};
use serde::Serialize;
use serde_json::{Value, json};

use crate::engine::types::NodeOutput;

use super::config::{McpAction, McpTransport};

fn serialize<T: Serialize>(value: &T) -> Value {
    serde_json::to_value(value).unwrap_or(Value::Null)
}

fn base(
    output_key: &str,
    transport: McpTransport,
    action: McpAction,
    session: &str,
    result: Value,
) -> NodeOutput {
    let mut output = NodeOutput::new();
    output.insert(
        format!("{output_key}_transport"),
        Value::String(transport.to_string()),
    );
    output.insert(
        format!("{output_key}_action"),
        Value::String(action.to_string()),
    );
    output.insert(
        format!("{output_key}_session"),
        Value::String(session.to_string()),
    );
    output.insert(format!("{output_key}_result"), result);
    output.insert(format!("{output_key}_success"), Value::Bool(true));
    output
}

pub(super) fn initialized(
    output_key: &str,
    transport: McpTransport,
    session: &str,
    info: &ServerPeerInfo,
) -> NodeOutput {
    let result = serialize(info);
    let mut output = base(
        output_key,
        transport,
        McpAction::Initialize,
        session,
        result,
    );
    output.insert(
        format!("{output_key}_protocol_version"),
        Value::String(info.protocol_version.to_string()),
    );
    output.insert(
        format!("{output_key}_capabilities"),
        serialize(&info.capabilities),
    );
    output.insert(
        format!("{output_key}_server_info"),
        serialize(&info.server_info),
    );
    output
}

pub(super) fn tools(
    output_key: &str,
    transport: McpTransport,
    session: &str,
    result: &ListToolsResult,
) -> NodeOutput {
    let mut output = base(
        output_key,
        transport,
        McpAction::ListTools,
        session,
        serialize(result),
    );
    let tools = serialize(&result.tools);
    let names = result
        .tools
        .iter()
        .map(|tool| Value::String(tool.name.to_string()))
        .collect::<Vec<_>>();
    output.insert(format!("{output_key}_tools"), tools);
    output.insert(format!("{output_key}_tool_count"), Value::from(names.len()));
    output.insert(format!("{output_key}_tool_names"), Value::Array(names));
    output
}

pub(super) fn tool_call(
    output_key: &str,
    transport: McpTransport,
    session: &str,
    tool_name: &str,
    result: &CallToolResult,
) -> NodeOutput {
    let result_value = serialize(result);
    let content = result_value.get("content").cloned().unwrap_or(Value::Null);
    let text = content.as_array().and_then(|blocks| {
        let fragments = blocks
            .iter()
            .filter_map(|block| block.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>();
        (!fragments.is_empty()).then(|| fragments.join("\n"))
    });

    let mut output = base(
        output_key,
        transport,
        McpAction::CallTool,
        session,
        result_value.clone(),
    );
    output.insert(
        format!("{output_key}_tool_name"),
        Value::String(tool_name.to_string()),
    );
    output.insert(format!("{output_key}_tool_result"), result_value);
    output.insert(format!("{output_key}_tool_content"), content);
    output.insert(
        format!("{output_key}_tool_text"),
        text.map(Value::String).unwrap_or(Value::Null),
    );
    output
}

pub(super) fn closed(output_key: &str, transport: McpTransport, session: &str) -> NodeOutput {
    let mut output = base(
        output_key,
        transport,
        McpAction::Close,
        session,
        json!({ "closed": true }),
    );
    output.insert(format!("{output_key}_closed"), Value::Bool(true));
    output
}
