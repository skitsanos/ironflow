use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;

use crate::engine::types::{Context, NodeOutput};
use crate::nodes::Node;

use super::protocol::{
    McpAction, McpTransport, action_from_config, build_request, check_rpc_response,
    interpolate_json_value, is_sse_auto_initialize_enabled, next_request_id, output_key,
    timeout_seconds, tool_text_from_content, transport_from_config,
};
use super::session::is_session_initialized;
use super::transport::{
    ensure_initialized_sse, execute_sse, execute_stdio, parse_transport_response,
    prepare_sse_headers,
};

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

    async fn execute(&self, config: &Value, ctx: &Context) -> Result<NodeOutput> {
        let config = interpolate_json_value(config, ctx);
        let transport = transport_from_config(&config)?;
        let action = action_from_config(&config)?;
        let output_key = output_key(&config).to_string();
        let timeout_s = timeout_seconds(&config);

        let request = build_request(action, &config, ctx)?;
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
                    .ok_or_else(|| anyhow::anyhow!("mcp_client sse requires 'url'"))?;

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
                        ctx,
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
            Value::Object(serde_json::Map::new())
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
