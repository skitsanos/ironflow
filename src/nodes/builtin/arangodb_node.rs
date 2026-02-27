use anyhow::Result;
use async_trait::async_trait;
use std::time::Duration;

use crate::engine::types::{Context, NodeOutput};
use crate::lua::interpolate::interpolate_ctx;
use crate::nodes::Node;

/// Recursively interpolate `${ctx.key}` in all string values within a JSON value.
fn interpolate_json_value(value: &serde_json::Value, ctx: &Context) -> serde_json::Value {
    match value {
        serde_json::Value::String(s) => serde_json::Value::String(interpolate_ctx(s, ctx)),
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(|v| interpolate_json_value(v, ctx)).collect())
        }
        serde_json::Value::Object(map) => {
            let new_map: serde_json::Map<String, serde_json::Value> = map
                .iter()
                .map(|(k, v)| (k.clone(), interpolate_json_value(v, ctx)))
                .collect();
            serde_json::Value::Object(new_map)
        }
        other => other.clone(),
    }
}

/// Resolve a config string parameter, falling back to an environment variable.
fn resolve_param(
    config: &serde_json::Value,
    key: &str,
    env_key: &str,
    ctx: &Context,
) -> Option<String> {
    config
        .get(key)
        .and_then(|v| v.as_str())
        .map(|s| interpolate_ctx(s, ctx))
        .or_else(|| std::env::var(env_key).ok())
}

pub struct ArangoDbAqlNode;

#[async_trait]
impl Node for ArangoDbAqlNode {
    fn node_type(&self) -> &str {
        "arangodb_aql"
    }

    fn description(&self) -> &str {
        "Execute an AQL query against ArangoDB via the Cursor API"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
        // Connection parameters (config overrides env)
        let url = resolve_param(config, "url", "ARANGODB_URL", &ctx).ok_or_else(|| {
            anyhow::anyhow!("arangodb_aql requires 'url' or ARANGODB_URL env var")
        })?;

        let database =
            resolve_param(config, "database", "ARANGODB_DATABASE", &ctx).ok_or_else(|| {
                anyhow::anyhow!("arangodb_aql requires 'database' or ARANGODB_DATABASE env var")
            })?;

        let query = config
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("arangodb_aql requires 'query' parameter"))?;

        let query = interpolate_ctx(query, &ctx);

        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .unwrap_or("aql");

        let timeout_s = config
            .get("timeout")
            .and_then(|v| v.as_f64())
            .unwrap_or(30.0);

        // Build the cursor API URL
        let base_url = url.trim_end_matches('/');
        let cursor_url = format!("{}/_db/{}/_api/cursor", base_url, database);

        // Build the request body
        let mut body = serde_json::json!({ "query": query });

        if let Some(bind_vars) = config.get("bindVars") {
            let interpolated = interpolate_json_value(bind_vars, &ctx);
            body["bindVars"] = interpolated;
        }

        if let Some(batch_size) = config.get("batchSize").and_then(|v| v.as_u64()) {
            body["batchSize"] = serde_json::json!(batch_size);
        }

        // Build HTTP client and request
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs_f64(timeout_s))
            .build()?;

        let mut request = client.post(&cursor_url);

        // Authentication: token (JWT Bearer) or username/password (Basic)
        let token = resolve_param(config, "token", "ARANGODB_TOKEN", &ctx);
        let username = resolve_param(config, "username", "ARANGODB_USERNAME", &ctx);
        let password = resolve_param(config, "password", "ARANGODB_PASSWORD", &ctx);

        if let Some(token) = token {
            request = request.bearer_auth(token);
        } else if let Some(username) = username {
            request = request.basic_auth(username, password);
        }

        // Execute
        let response = request
            .json(&body)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("ArangoDB request failed: {}", e))?;

        let status = response.status();
        let response_body: serde_json::Value = response
            .json()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to parse ArangoDB response: {}", e))?;

        if !status.is_success() {
            let error_msg = response_body
                .get("errorMessage")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown error");
            let error_num = response_body
                .get("errorNum")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            anyhow::bail!(
                "ArangoDB error {}: {} (HTTP {})",
                error_num,
                error_msg,
                status
            );
        }

        // Extract results
        let result = response_body
            .get("result")
            .cloned()
            .unwrap_or(serde_json::Value::Array(vec![]));

        let count = match &result {
            serde_json::Value::Array(arr) => arr.len(),
            _ => 0,
        };

        let has_more = response_body
            .get("hasMore")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let mut output = NodeOutput::new();
        output.insert(format!("{}_result", output_key), result);
        output.insert(format!("{}_count", output_key), serde_json::json!(count));
        output.insert(
            format!("{}_has_more", output_key),
            serde_json::Value::Bool(has_more),
        );

        // Include stats if available
        if let Some(extra) = response_body.get("extra")
            && let Some(stats) = extra.get("stats")
        {
            output.insert(format!("{}_stats", output_key), stats.clone());
        }

        output.insert(
            format!("{}_success", output_key),
            serde_json::Value::Bool(true),
        );

        Ok(output)
    }
}
