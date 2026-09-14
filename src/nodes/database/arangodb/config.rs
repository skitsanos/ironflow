use anyhow::{Result, bail};
use serde_json::{Value, json};

use crate::engine::types::Context;
use crate::lua::interpolate::{interpolate_ctx, interpolate_value};
use crate::util::duration::positive_duration;
use crate::util::node_config::{config_f64_strict, config_u64_strict};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Action {
    Query,
    Next,
    Close,
}

pub(super) struct Operation {
    pub action: Action,
    pub cursor_id: Option<String>,
    pub body: Option<Value>,
}

impl Operation {
    pub(super) fn parse(config: &Value, ctx: &Context) -> Result<Self> {
        let action = match config.get("action") {
            None => Action::Query,
            Some(Value::String(value)) => match interpolate_ctx(value, ctx).as_str() {
                "query" => Action::Query,
                "next" => Action::Next,
                "close" => Action::Close,
                _ => bail!("arangodb_aql action must be query, next, or close"),
            },
            _ => bail!("arangodb_aql action must be query, next, or close"),
        };
        if action != Action::Query {
            let id = config
                .get("cursor_id")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    anyhow::anyhow!("arangodb_aql requires string cursor_id for next/close")
                })?;
            let id = interpolate_ctx(id, ctx);
            validate_cursor(&id)?;
            return Ok(Self {
                action,
                cursor_id: Some(id),
                body: None,
            });
        }
        let query = config
            .get("query")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow::anyhow!("arangodb_aql requires 'query' parameter"))?;
        if query.contains("${ctx") {
            bail!(
                "arangodb_aql: the query must not interpolate context values with \
                '${{ctx...}}' (AQL injection risk). Supply runtime values via \
                'bindVars' with @var placeholders instead."
            );
        }
        let mut body = json!({"query": interpolate_ctx(query, ctx)});
        if let Some(bind_vars) = config.get("bindVars") {
            if !bind_vars.is_object() {
                bail!("arangodb_aql bindVars must be an object");
            }
            body["bindVars"] = interpolate_value(bind_vars, ctx);
        }
        if let Some(batch_size) = config_u64_strict(config, "batchSize", ctx)? {
            if batch_size == 0 {
                bail!("arangodb_aql batchSize must be positive");
            }
            body["batchSize"] = json!(batch_size);
        }
        if let Some(ttl) = config_f64_strict(config, "ttl", ctx)? {
            positive_duration(ttl, "arangodb_aql ttl")?;
            body["ttl"] = json!(ttl);
        }
        Ok(Self {
            action,
            cursor_id: None,
            body: Some(body),
        })
    }
}

fn validate_cursor(id: &str) -> Result<()> {
    if id.is_empty()
        || id.len() > 128
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        bail!(
            "arangodb_aql cursor_id must contain 1-128 ASCII letters, digits, underscores, or hyphens"
        );
    }
    Ok(())
}

pub(super) fn response_cursor(body: &Value) -> Result<Option<&str>> {
    match body.get("id") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(id)) => {
            validate_cursor(id)?;
            Ok(Some(id))
        }
        _ => bail!("ArangoDB response cursor id must be a string"),
    }
}

pub(super) fn resolve_param(
    config: &Value,
    key: &str,
    env_key: &str,
    ctx: &Context,
) -> Option<String> {
    config
        .get(key)
        .and_then(Value::as_str)
        .map(|s| interpolate_ctx(s, ctx))
        .or_else(|| std::env::var(env_key).ok())
}
