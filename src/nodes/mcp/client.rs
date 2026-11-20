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
use super::result_limit;
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

        if action == McpAction::Initialize {
            let transport = transport(&config)?;
            let mut session =
                mcp_transport::initialize(&config, client_info(&config)?, transport, timeout)
                    .await?;
            if let Err(error) = session.validate_protocol() {
                let _ = session.close(timeout).await;
                return Err(error);
            }
            let info = session.server_info()?;
            // The server's own description becomes context like any
            // other result, so an oversized one ends the session.
            if let Err(error) = result_limit::admit(action, &info) {
                let _ = session.close(timeout).await;
                return Err(error);
            }
            let handle = self.sessions.insert(session)?;
            return Ok(output::initialized(&output_key, transport, &handle, &info));
        }
        let deadline = tokio::time::sleep(timeout).deadline();
        let operation = async {
            let handle = session_handle(&config)?;
            match action {
                McpAction::Initialize => unreachable!(),
                McpAction::ListTools => {
                    let cursor = config
                        .get("cursor")
                        .map(|value| {
                            value.as_str().ok_or_else(|| {
                                anyhow::anyhow!(
                                    "mcp_client list_tools expects 'cursor' to be a string"
                                )
                            })
                        })
                        .transpose()?;
                    // An absent `nextCursor` interpolates to an empty string.
                    // Sent as a cursor it is rejected by a strict server, which
                    // ends the session, or answered with the first page again.
                    // Refuse it before the session is leased (IF-261).
                    anyhow::ensure!(
                        cursor != Some(""),
                        "mcp_client list_tools received an empty 'cursor': the previous page \
                         had no nextCursor, so there is no further page; run this step only \
                         when nextCursor is present"
                    );
                    let mut lease = self.sessions.lease(handle, deadline).await?;
                    let session = lease.session();
                    let transport = session.transport();
                    let result = session.list_tools(cursor, timeout).await?;
                    lease.disarm();
                    // The reply was complete, so the session stays usable.
                    result_limit::admit(action, &result)?;
                    Ok(output::tools(&output_key, transport, handle, &result))
                }
                McpAction::CallTool => {
                    let (tool_name, arguments) = tool_call(&config)?;
                    let mut lease = self.sessions.lease(handle, deadline).await?;
                    let session = lease.session();
                    let transport = session.transport();
                    let result = session
                        .call_tool(tool_name.clone(), arguments, timeout)
                        .await?;
                    lease.disarm();
                    result_limit::admit(action, &result)?;
                    Ok(output::tool_call(
                        &output_key,
                        transport,
                        handle,
                        &tool_name,
                        &result,
                    ))
                }
                McpAction::Close => {
                    let mut lease = self.sessions.lease(handle, deadline).await?;
                    let session = lease.session();
                    let transport = session.transport();
                    session.close(timeout).await?;
                    Ok(output::closed(&output_key, transport, handle))
                }
            }
        };
        tokio::time::timeout_at(deadline, operation)
            .await
            .map_err(|_| anyhow::anyhow!("mcp_client: {action} timed out"))?
    }
}
