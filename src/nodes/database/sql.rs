use anyhow::Result;
use async_trait::async_trait;
use futures_util::TryStreamExt;
use sqlx::{AnyPool, Arguments};

use super::row::row_to_json;

use crate::engine::types::{Context, NodeOutput};
use crate::lua::interpolate::interpolate_ctx;
use crate::nodes::Node;
use crate::util::limits;
use crate::util::node_config::config_u64;
use crate::util::sensitive_url::{Connection, redact_sensitive_text};

/// Reject `${ctx...}` interpolation inside a SQL query body.
///
/// The query text is passed to `sqlx::AssertSqlSafe`, which bypasses sqlx's
/// compile-time SQL-safety guard, so interpolating a runtime value directly
/// into the query is a SQL-injection vector. Runtime values must be supplied
/// through the `params` array and bound as `?`/`$1` placeholders instead.
fn reject_query_interpolation(query: &str, node: &str) -> Result<()> {
    if query.contains("${ctx") {
        anyhow::bail!(
            "{node}: the query must not interpolate context values with '${{ctx...}}' \
             (SQL injection risk). Supply runtime values via the 'params' array with \
             '?'/'$1' placeholders instead."
        );
    }
    Ok(())
}

/// Resolve query parameters from config with context interpolation,
/// preserving JSON types (string, number, bool, null) for proper SQL binding.
fn resolve_params(config: &serde_json::Value, ctx: &Context) -> Vec<serde_json::Value> {
    config
        .get("params")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .map(|v| match v {
                    serde_json::Value::String(s) => {
                        let interpolated = interpolate_ctx(s, ctx);
                        serde_json::Value::String(interpolated)
                    }
                    other => other.clone(),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn optional_u64_config(config: &serde_json::Value, key: &str, ctx: &Context) -> Option<u64> {
    config_u64(config, key, ctx)
}

/// Bind typed JSON parameters to an sqlx AnyArguments buffer.
pub(super) fn bind_params(params: &[serde_json::Value]) -> Result<sqlx::any::AnyArguments> {
    let mut args = sqlx::any::AnyArguments::default();
    for (i, param) in params.iter().enumerate() {
        match param {
            serde_json::Value::String(s) => args
                .add(s.as_str())
                .map_err(|e| anyhow::anyhow!("Failed to bind param {}: {}", i, e))?,
            serde_json::Value::Number(n) => {
                if let Some(int_val) = n.as_i64() {
                    args.add(int_val)
                        .map_err(|e| anyhow::anyhow!("Failed to bind param {}: {}", i, e))?;
                } else if let Some(float_val) = n.as_f64() {
                    args.add(float_val)
                        .map_err(|e| anyhow::anyhow!("Failed to bind param {}: {}", i, e))?;
                }
            }
            serde_json::Value::Bool(b) => args
                .add(*b)
                .map_err(|e| anyhow::anyhow!("Failed to bind param {}: {}", i, e))?,
            serde_json::Value::Null => args
                .add(None::<String>)
                .map_err(|e| anyhow::anyhow!("Failed to bind param {}: {}", i, e))?,
            _ => anyhow::bail!(
                "Unsupported param type at index {}: arrays/objects cannot be bound as SQL parameters",
                i
            ),
        }
    }
    Ok(args)
}

/// Connect to a database using the `connection` config parameter.
pub(super) async fn connect(config: &serde_json::Value, ctx: &Context) -> Result<AnyPool> {
    let url = config
        .get("connection")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("db node requires 'connection' (database URL string)"))?;

    let url = interpolate_ctx(url, ctx);

    // Install any drivers that are compiled in, and the Rustls provider the
    // PostgreSQL `verify-ca` verifier needs (idempotent; see IF-141).
    sqlx::any::install_default_drivers();
    crate::initialize_tls_provider();

    let pool = AnyPool::connect(&url).await.map_err(|_| {
        // Drivers may detach credentials or query values from the URL and
        // repeat them in diagnostics, so a generic text scrubber cannot make
        // the cause safe to expose. Keep the redacted endpoint and fail closed.
        anyhow::anyhow!("Failed to connect to database at {}", Connection::new(&url))
    })?;

    Ok(pool)
}

pub struct DbQueryNode;

#[async_trait]
impl Node for DbQueryNode {
    fn node_type(&self) -> &str {
        "db_query"
    }

    fn description(&self) -> &str {
        "Execute a SELECT query and return rows as JSON"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let query = config
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("db_query requires 'query' parameter"))?;

        reject_query_interpolation(query, "db_query")?;
        let query = interpolate_ctx(query, ctx);
        let params = resolve_params(config, ctx);
        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .unwrap_or("rows");
        let max_rows = optional_u64_config(config, "max_rows", ctx)
            .filter(|limit| *limit > 0)
            .or_else(limits::max_db_rows);
        let max_result_bytes = optional_u64_config(config, "max_result_bytes", ctx)
            .filter(|limit| *limit > 0)
            .or_else(limits::max_db_result_bytes);

        let pool = connect(config, ctx).await?;
        let args = bind_params(&params)?;

        let mut stream = sqlx::query_with(sqlx::AssertSqlSafe(query.as_str()), args).fetch(&pool);
        let mut json_rows = Vec::new();
        let mut serialized_bytes = 2u64; // '[' + ']'

        while let Some(row) = stream.try_next().await.map_err(|error| {
            anyhow::anyhow!(
                "db_query failed: {}",
                redact_sensitive_text(&error.to_string())
            )
        })? {
            if let Some(max_rows) = max_rows
                && json_rows.len() as u64 >= max_rows
            {
                anyhow::bail!(
                    "db_query exceeded max_rows limit of {}. Add pagination or raise max_rows / IRONFLOW_DB_MAX_ROWS.",
                    max_rows
                );
            }

            let json_row = row_to_json(&row)?;
            let row_bytes = serde_json::to_vec(&json_row)?.len() as u64;
            let separator_bytes = u64::from(!json_rows.is_empty());
            let next_size = serialized_bytes + row_bytes + separator_bytes;
            if let Some(max_result_bytes) = max_result_bytes
                && next_size > max_result_bytes
            {
                anyhow::bail!(
                    "db_query exceeded max_result_bytes limit of {}. Add pagination or raise max_result_bytes / IRONFLOW_DB_MAX_RESULT_BYTES.",
                    max_result_bytes
                );
            }

            serialized_bytes = next_size;
            json_rows.push(json_row);
        }

        let count = json_rows.len();

        let mut output = NodeOutput::new();
        output.insert(output_key.to_string(), serde_json::Value::Array(json_rows));
        output.insert(format!("{}_count", output_key), serde_json::json!(count));
        output.insert(
            format!("{}_success", output_key),
            serde_json::Value::Bool(true),
        );
        Ok(output)
    }
}

pub struct DbExecNode;

#[async_trait]
impl Node for DbExecNode {
    fn node_type(&self) -> &str {
        "db_exec"
    }

    fn description(&self) -> &str {
        "Execute an INSERT, UPDATE, or DELETE statement"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let query = config
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("db_exec requires 'query' parameter"))?;

        reject_query_interpolation(query, "db_exec")?;
        let query = interpolate_ctx(query, ctx);
        let params = resolve_params(config, ctx);

        let pool = connect(config, ctx).await?;
        let args = bind_params(&params)?;

        let result = sqlx::query_with(sqlx::AssertSqlSafe(query.as_str()), args)
            .execute(&pool)
            .await
            .map_err(|error| {
                anyhow::anyhow!(
                    "db_exec failed: {}",
                    redact_sensitive_text(&error.to_string())
                )
            })?;

        let rows_affected = result.rows_affected();

        let mut output = NodeOutput::new();
        output.insert(
            "rows_affected".to_string(),
            serde_json::json!(rows_affected),
        );
        output.insert("db_exec_success".to_string(), serde_json::Value::Bool(true));
        Ok(output)
    }
}
