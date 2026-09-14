mod client;
mod config;

use anyhow::{Result, bail};
use async_trait::async_trait;
use serde_json::{Value, json};

use crate::engine::types::{Context, NodeOutput};
use crate::nodes::Node;
use client::{CursorClient, check_response};
use config::{Action, Operation, response_cursor};

pub struct ArangoDbAqlNode;

#[async_trait]
impl Node for ArangoDbAqlNode {
    fn node_type(&self) -> &str {
        "arangodb_aql"
    }

    fn description(&self) -> &str {
        "Query, continue, or close an ArangoDB AQL cursor"
    }

    async fn execute(&self, config: &Value, ctx: &Context) -> Result<NodeOutput> {
        let operation = Operation::parse(config, ctx)?;
        let client = CursorClient::new(config, ctx)?;
        let mut cleanup = client.cleanup(operation.cursor_id.as_deref());
        let (status, body) = client.execute(&operation).await?;
        let cursor = if operation.action == Action::Close {
            None
        } else {
            let cursor = response_cursor(&body)?;
            if let Some(existing) = operation.cursor_id.as_deref() {
                if cursor.is_some_and(|id| id != existing) {
                    bail!("ArangoDB response changed cursor identity");
                }
            } else if let Some(cursor) = cursor {
                // Arm cleanup before validating the rest of the provider response.
                cleanup = client.cleanup(Some(cursor));
            }
            cursor
        };
        check_response(status, &body, operation.action == Action::Close)?;

        let closed = operation.action == Action::Close;
        let (rows, has_more) = if closed {
            (Vec::new(), false)
        } else {
            let rows = body
                .get("result")
                .and_then(Value::as_array)
                .ok_or_else(|| anyhow::anyhow!("ArangoDB response requires a result array"))?;
            let has_more = body
                .get("hasMore")
                .and_then(Value::as_bool)
                .ok_or_else(|| anyhow::anyhow!("ArangoDB response requires boolean hasMore"))?;
            if has_more && cursor.is_none() {
                bail!("ArangoDB response hasMore=true requires a cursor id");
            }
            (rows.clone(), has_more)
        };
        let prefix = config
            .get("output_key")
            .and_then(Value::as_str)
            .unwrap_or("aql");
        let mut output = NodeOutput::new();
        output.insert(format!("{prefix}_count"), json!(rows.len()));
        output.insert(format!("{prefix}_result"), Value::Array(rows));
        output.insert(format!("{prefix}_has_more"), json!(has_more));
        output.insert(format!("{prefix}_cursor_id"), json!(cursor));
        output.insert(format!("{prefix}_closed"), json!(closed));
        output.insert(
            format!("{prefix}_stats"),
            if closed {
                Value::Null
            } else {
                body.pointer("/extra/stats").cloned().unwrap_or(Value::Null)
            },
        );
        output.insert(format!("{prefix}_success"), json!(true));
        cleanup.disarm();
        Ok(output)
    }
}
