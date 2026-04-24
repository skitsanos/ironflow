use anyhow::Result;
use async_trait::async_trait;
use futures_util::TryStreamExt;
use sqlx::any::AnyRow;
use sqlx::{AnyPool, Arguments, Column, Row, TypeInfo};

use crate::engine::types::{Context, NodeOutput};
use crate::lua::interpolate::interpolate_ctx;
use crate::nodes::Node;
use crate::util::limits;

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

fn optional_u64_config(config: &serde_json::Value, key: &str) -> Option<u64> {
    config.get(key).and_then(|value| match value {
        serde_json::Value::Number(n) => n.as_u64(),
        serde_json::Value::String(s) => s.parse::<u64>().ok(),
        _ => None,
    })
}

/// Bind typed JSON parameters to an sqlx AnyArguments buffer.
fn bind_params(params: &[serde_json::Value]) -> Result<sqlx::any::AnyArguments<'_>> {
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

/// Convert a row to a JSON object by inspecting column types at runtime.
fn row_to_json(row: &AnyRow) -> Result<serde_json::Value> {
    let mut map = serde_json::Map::new();

    for col in row.columns() {
        let name = col.name().to_string();
        let type_name = col.type_info().name();

        let value: serde_json::Value = match type_name {
            "INTEGER" | "INT" | "INT4" | "INT8" | "BIGINT" | "SMALLINT" => {
                match row.try_get::<i64, _>(col.ordinal()) {
                    Ok(v) => serde_json::json!(v),
                    Err(_) => serde_json::Value::Null,
                }
            }
            "REAL" | "FLOAT" | "FLOAT4" | "FLOAT8" | "DOUBLE" | "NUMERIC" => {
                match row.try_get::<f64, _>(col.ordinal()) {
                    Ok(v) => serde_json::json!(v),
                    Err(_) => serde_json::Value::Null,
                }
            }
            "BOOLEAN" | "BOOL" => {
                if let Ok(v) = row.try_get::<bool, _>(col.ordinal()) {
                    serde_json::json!(v)
                } else if let Ok(v) = row.try_get::<i64, _>(col.ordinal()) {
                    serde_json::json!(v != 0)
                } else if let Ok(v) = row.try_get::<f64, _>(col.ordinal()) {
                    serde_json::json!(v != 0.0)
                } else if let Ok(v) = row.try_get::<String, _>(col.ordinal()) {
                    matches!(
                        v.as_str(),
                        "1" | "true" | "TRUE" | "True" | "t" | "T" | "yes" | "YES"
                    )
                    .then_some(serde_json::Value::Bool(true))
                    .unwrap_or(serde_json::Value::Bool(false))
                } else {
                    serde_json::Value::Null
                }
            }
            _ => {
                // Default: try as string (TEXT, VARCHAR, etc.)
                match row.try_get::<String, _>(col.ordinal()) {
                    Ok(v) => serde_json::Value::String(v),
                    Err(_) => serde_json::Value::Null,
                }
            }
        };

        map.insert(name, value);
    }

    Ok(serde_json::Value::Object(map))
}

/// Connect to a database using the `connection` config parameter.
async fn connect(config: &serde_json::Value, ctx: &Context) -> Result<AnyPool> {
    let url = config
        .get("connection")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("db node requires 'connection' (database URL string)"))?;

    let url = interpolate_ctx(url, ctx);

    // Install any drivers that are compiled in
    sqlx::any::install_default_drivers();

    let pool = AnyPool::connect(&url)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to connect to database '{}': {}", url, e))?;

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

        let query = interpolate_ctx(query, ctx);
        let params = resolve_params(config, ctx);
        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .unwrap_or("rows");
        let max_rows = optional_u64_config(config, "max_rows")
            .filter(|limit| *limit > 0)
            .or_else(limits::max_db_rows);
        let max_result_bytes = optional_u64_config(config, "max_result_bytes")
            .filter(|limit| *limit > 0)
            .or_else(limits::max_db_result_bytes);

        let pool = connect(config, ctx).await?;
        let args = bind_params(&params)?;

        let mut stream = sqlx::query_with(&query, args).fetch(&pool);
        let mut json_rows = Vec::new();
        let mut serialized_bytes = 2u64; // '[' + ']'

        while let Some(row) = stream
            .try_next()
            .await
            .map_err(|e| anyhow::anyhow!("db_query failed: {}", e))?
        {
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

        let query = interpolate_ctx(query, ctx);
        let params = resolve_params(config, ctx);

        let pool = connect(config, ctx).await?;
        let args = bind_params(&params)?;

        let result = sqlx::query_with(&query, args)
            .execute(&pool)
            .await
            .map_err(|e| anyhow::anyhow!("db_exec failed: {}", e))?;

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
