//! The metadata filter of `s3vector_query_vectors`.
//!
//! A filter scopes the query, so neither form may fall back to something
//! wider: a `filter_key` must resolve to a filter (IF-192) and every
//! placeholder of an inline `filter` must resolve to a value (IF-221).

use anyhow::Result;
use aws_smithy_types::Document;
use serde_json::Value;

use crate::engine::types::Context;
use crate::lua::interpolate::{interpolate_ctx, interpolate_ctx_required};

use super::document::parse_metadata;

const NODE: &str = "s3vector_query_vectors";

/// The request filter: the inline `filter` when set, else the value named by
/// `filter_key`, else none.
pub(super) fn resolve_filter(config: &Value, ctx: &Context) -> Result<Option<Document>> {
    match config.get("filter") {
        Some(filter) => {
            let mut entry = String::from("filter");
            let filter = interpolate_filter(filter, ctx, &mut entry)?;
            parse_metadata(&filter, NODE).map(Some)
        }
        None => resolve_context_filter(config, ctx),
    }
}

/// Resolve the placeholders of an inline filter once. Strings are templates
/// because the filter is written in the flow; keys are structural and stay
/// as written, and an interpolated value is not read again. `entry` is the
/// dotted position reported when a placeholder does not resolve.
fn interpolate_filter(value: &Value, ctx: &Context, entry: &mut String) -> Result<Value> {
    Ok(match value {
        Value::String(template) => {
            Value::String(interpolate_ctx_required(template, ctx).map_err(|error| {
                anyhow::anyhow!("{NODE} could not interpolate '{entry}': {error}")
            })?)
        }
        Value::Array(items) => Value::Array(
            items
                .iter()
                .enumerate()
                .map(|(index, item)| {
                    at(entry, &index.to_string(), |entry| {
                        interpolate_filter(item, ctx, entry)
                    })
                })
                .collect::<Result<_>>()?,
        ),
        Value::Object(fields) => Value::Object(
            fields
                .iter()
                .map(|(key, item)| {
                    let item = at(entry, key, |entry| interpolate_filter(item, ctx, entry))?;
                    Ok((key.clone(), item))
                })
                .collect::<Result<_>>()?,
        ),
        other => other.clone(),
    })
}

/// Run `visit` with `segment` appended to the entry path.
fn at<T>(entry: &mut String, segment: &str, visit: impl FnOnce(&mut String) -> T) -> T {
    let length = entry.len();
    entry.push('.');
    entry.push_str(segment);
    let result = visit(entry);
    entry.truncate(length);
    result
}

/// Reads the filter named by `filter_key`. A configured key is a requirement:
/// a missing key or a value that restricts nothing fails instead of widening
/// the query to the whole index. Errors name the key, never its value. The
/// stored filter is data and is not interpolated.
fn resolve_context_filter(config: &Value, ctx: &Context) -> Result<Option<Document>> {
    let Some(filter_key) = config.get("filter_key") else {
        return Ok(None);
    };
    let key = filter_key
        .as_str()
        .map(|key| interpolate_ctx(key, ctx))
        .filter(|key| !key.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("{NODE} requires 'filter_key' to be a non-empty string"))?;
    let value = ctx
        .get(&key)
        .ok_or_else(|| anyhow::anyhow!("{NODE} requires filter_key '{key}' to exist in context"))?;
    if !value.as_object().is_some_and(|filter| !filter.is_empty()) {
        anyhow::bail!("{NODE} requires filter_key '{key}' to hold a non-empty filter object");
    }
    parse_metadata(value, NODE).map(Some)
}
