use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;

use crate::engine::types::{Context, NodeOutput};
use crate::nodes::Node;

use super::config::{
    McpAction, action, client_info, interpolate_config, output_key, session_handle, timeout,
    tool_call, transport,
};
use super::output;
use super::session::SessionManager;
use super::transport as mcp_transport;

#[derive(Default)]
pub struct McpClientNode {
    sessions: Arc<SessionManager>,
}

#[async_trait]
impl Node for McpClientNode {
    fn node_type(&self) -> &str {
        "mcp_client"
    }

    fn description(&self) -> &str {
        "Stateful MCP 2025-11-25 client over stdio or Streamable HTTP"
    }

    async fn execute(&self, config: &Value, context: &Context) -> Result<NodeOutput> {
        let config = interpolate_config(config, context);
        let action = action(&config)?;
        let output_key = output_key(&config).to_string();
        let timeout = timeout(&config)?;

        match action {
            McpAction::Initialize => {
                let transport = transport(&config)?;
                let mut session =
                    mcp_transport::initialize(&config, client_info(&config)?, transport, timeout)
                        .await?;
                if let Err(error) = session.validate_protocol() {
                    let _ = session.close(timeout).await;
                    return Err(error);
                }
                let info = session.server_info()?;
                let handle = self.sessions.insert(session)?;
                Ok(output::initialized(&output_key, transport, &handle, &info))
            }
            McpAction::ListTools => {
                let handle = session_handle(&config)?.to_string();
                let mut lease = self.sessions.lease(&handle)?;
                let session = lease.session();
                let session = session.lock().await;
                let transport = session.transport();
                let result = session.list_tools(timeout).await?;
                drop(session);
                lease.disarm();
                Ok(output::tools(&output_key, transport, &handle, &result))
            }
            McpAction::CallTool => {
                let handle = session_handle(&config)?.to_string();
                let (tool_name, arguments) = tool_call(&config)?;
                let mut lease = self.sessions.lease(&handle)?;
                let session = lease.session();
                let session = session.lock().await;
                let transport = session.transport();
                let result = session
                    .call_tool(tool_name.clone(), arguments, timeout)
                    .await?;
                drop(session);
                lease.disarm();
                Ok(output::tool_call(
                    &output_key,
                    transport,
                    &handle,
                    &tool_name,
                    &result,
                ))
            }
            McpAction::Close => {
                let handle = session_handle(&config)?.to_string();
                let transport = self.sessions.close(&handle, timeout).await?;
                Ok(output::closed(&output_key, transport, &handle))
            }
        }
    }
}
