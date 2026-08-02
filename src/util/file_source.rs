use anyhow::Result;
use serde_json::Value;

use crate::artifacts::{ArtifactRef, FileSource};
use crate::engine::types::Context;
use crate::lua::interpolate::interpolate_ctx;

/// Parse a node's mutually exclusive `path` or `source_key` without touching
/// the filesystem. Artifact resolution is deliberately deferred to the
/// tracked blocking worker that consumes the returned source.
pub(crate) fn get_file_source(
    config: &Value,
    ctx: &Context,
    node_name: &str,
) -> Result<FileSource> {
    let path = optional_string(config, "path", node_name)?;
    let source_key = optional_string(config, "source_key", node_name)?;
    if path.is_some() && source_key.is_some() {
        anyhow::bail!("{node_name} accepts either 'path' or 'source_key', not both");
    }

    if let Some(path) = path {
        return parse_file_source(&Value::String(interpolate_ctx(path, ctx)), ctx, node_name);
    }
    if let Some(source_key) = source_key {
        let value = ctx
            .get(source_key)
            .ok_or_else(|| anyhow::anyhow!("Key '{source_key}' not found in context"))?;
        return parse_file_source(value, ctx, node_name).map_err(|error| {
            anyhow::anyhow!("Context key '{source_key}' must be a file path or artifact: {error}")
        });
    }
    anyhow::bail!("{node_name} requires either 'path' or 'source_key'")
}

pub(crate) fn parse_file_source(
    value: &Value,
    ctx: &Context,
    _node_name: &str,
) -> Result<FileSource> {
    match value {
        Value::String(value) => {
            let value = interpolate_ctx(value, ctx);
            if value.starts_with("artifact://") {
                ArtifactRef::validate_uri(&value)?;
                Ok(FileSource::artifact_uri(value))
            } else {
                Ok(FileSource::path(value))
            }
        }
        Value::Object(object) => {
            if let Some(artifact) = object.get("artifact") {
                if object.contains_key("artifact_uri") || object.len() != 1 {
                    anyhow::bail!("ambiguous artifact wrapper contains other source fields");
                }
                return parse_file_source(artifact, ctx, _node_name);
            }
            let artifact = ArtifactRef::from_value(value)
                .map_err(|error| anyhow::anyhow!("invalid artifact descriptor: {error}"))?;
            Ok(FileSource::artifact(artifact))
        }
        _ => anyhow::bail!("expected a string or artifact descriptor object"),
    }
}

fn optional_string<'a>(config: &'a Value, key: &str, node_name: &str) -> Result<Option<&'a str>> {
    match config.get(key) {
        None => Ok(None),
        Some(Value::String(value)) => Ok(Some(value)),
        Some(_) => anyhow::bail!("{node_name}: '{key}' must be a string"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parsing_preserves_artifact_identity_without_accessing_the_store() {
        let digest = "a".repeat(64);
        let descriptor = serde_json::json!({
            "artifact_uri": format!("artifact://sha256/{digest}"),
            "sha256": digest,
            "size_bytes": 7
        });
        let context = Context::from([("source".to_owned(), descriptor)]);
        let source = get_file_source(
            &serde_json::json!({"source_key": "source"}),
            &context,
            "test",
        )
        .unwrap();
        assert!(matches!(source, FileSource::Artifact(_)));
    }

    #[test]
    fn rejects_ambiguous_or_missing_sources() {
        let context = Context::new();
        let both = serde_json::json!({"path": "a", "source_key": "b"});
        assert!(get_file_source(&both, &context, "test").is_err());
        assert!(get_file_source(&serde_json::json!({}), &context, "test").is_err());
    }
}
