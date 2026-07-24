use anyhow::Result;
use async_trait::async_trait;

use crate::engine::types::{Context, NodeOutput};
use crate::lua::interpolate::{interpolate_ctx, interpolate_value};
use crate::nodes::Node;
use crate::util::duration::positive_duration;
use crate::util::node_config::{config_f64_or, config_u64};
use crate::util::sensitive_url::{SecretEndpoint, redact_sensitive_text};

/// Recursively interpolate context templates in all JSON string values.
fn interpolate_json_value(value: &serde_json::Value, ctx: &Context) -> serde_json::Value {
    interpolate_value(value, ctx)
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

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        // Connection parameters (config overrides env)
        let url = resolve_param(config, "url", "ARANGODB_URL", ctx).ok_or_else(|| {
            anyhow::anyhow!("arangodb_aql requires 'url' or ARANGODB_URL env var")
        })?;

        let database =
            resolve_param(config, "database", "ARANGODB_DATABASE", ctx).ok_or_else(|| {
                anyhow::anyhow!("arangodb_aql requires 'database' or ARANGODB_DATABASE env var")
            })?;

        let query = config
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("arangodb_aql requires 'query' parameter"))?;

        let query = interpolate_ctx(query, ctx);

        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .unwrap_or("aql");

        let timeout_s = config_f64_or(config, "timeout", ctx, 30.0)?;

        // Build the cursor API URL
        let base_url = url.trim_end_matches('/');
        let cursor_url = format!("{}/_db/{}/_api/cursor", base_url, database);

        // Build the request body
        let mut body = serde_json::json!({ "query": query });

        if let Some(bind_vars) = config.get("bindVars") {
            let interpolated = interpolate_json_value(bind_vars, ctx);
            body["bindVars"] = interpolated;
        }

        if let Some(batch_size) = config_u64(config, "batchSize", ctx) {
            body["batchSize"] = serde_json::json!(batch_size);
        }

        // Build HTTP client and request
        let client = reqwest::Client::builder()
            .timeout(positive_duration(timeout_s, "arangodb_aql timeout")?)
            .build()
            .map_err(|error| {
                anyhow::anyhow!(
                    "Failed to build ArangoDB client: {}",
                    redact_sensitive_text(&error.to_string())
                )
            })?;

        let mut request = client.post(&cursor_url);

        // Authentication: token (JWT Bearer) or username/password (Basic)
        let token = resolve_param(config, "token", "ARANGODB_TOKEN", ctx);
        let username = resolve_param(config, "username", "ARANGODB_USERNAME", ctx);
        let password = resolve_param(config, "password", "ARANGODB_PASSWORD", ctx);

        if let Some(token) = token {
            request = request.bearer_auth(token);
        } else if let Some(username) = username {
            request = request.basic_auth(username, password);
        }

        // Execute
        let response = request.json(&body).send().await.map_err(|error| {
            anyhow::anyhow!(
                "ArangoDB request to {} failed: {}",
                SecretEndpoint::new(&cursor_url),
                redact_sensitive_text(&error.to_string())
            )
        })?;

        let status = response.status();
        let response_body: serde_json::Value = response.json().await.map_err(|error| {
            anyhow::anyhow!(
                "Failed to parse ArangoDB response from {}: {}",
                SecretEndpoint::new(&cursor_url),
                redact_sensitive_text(&error.to_string())
            )
        })?;

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
                redact_sensitive_text(error_msg),
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
