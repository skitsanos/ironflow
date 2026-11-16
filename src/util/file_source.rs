use anyhow::Result;
use serde_json::Value;

use crate::artifacts::{ArtifactRef, FileSource};
use crate::engine::types::Context;
use crate::lua::interpolate::interpolate_ctx;

/// Parse a node's mutually exclusive `path` or `source_key` without touching
/// the filesystem. Artifact resolution is deliberately deferred to the
/// tracked blocking worker that consumes the returned source.
///
/// `path` is configuration and is interpolated exactly once. The value behind
/// `source_key` is context data and is never treated as a template.
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
        return source_from_string(interpolate_ctx(path, ctx));
    }
    if let Some(source_key) = source_key {
        let value = ctx
            .get(source_key)
            .ok_or_else(|| anyhow::anyhow!("Key '{source_key}' not found in context"))?;
        return parse_file_source(value).map_err(|error| {
            anyhow::anyhow!("Context key '{source_key}' must be a file path or artifact: {error}")
        });
    }
    anyhow::bail!("{node_name} requires either 'path' or 'source_key'")
}

/// Where a source value was written, which decides whether its strings are
/// templates.
#[derive(Clone, Copy)]
pub(crate) enum SourceOrigin<'a> {
    /// Node configuration: `${ctx...}` placeholders are resolved once.
    Config(&'a Context),
    /// Context data produced by an earlier step: used exactly as stored.
    Data,
}

impl SourceOrigin<'_> {
    fn text(self, value: &str) -> String {
        match self {
            Self::Config(ctx) => interpolate_ctx(value, ctx),
            Self::Data => value.to_owned(),
        }
    }
}

/// Parse a path, artifact URI or artifact descriptor that is already data.
/// Nothing in it is interpolated.
pub(crate) fn parse_file_source(value: &Value) -> Result<FileSource> {
    parse_source(value, SourceOrigin::Data, true)
}

/// Parse a source value from `origin`; a configured string is interpolated
/// once and its result is never expanded again.
pub(crate) fn parse_file_source_from(
    value: &Value,
    origin: SourceOrigin<'_>,
) -> Result<FileSource> {
    parse_source(value, origin, true)
}

/// Parse an input that must name a stored artifact: a descriptor, a canonical
/// `artifact://sha256/...` URI, or an `{ artifact = ... }` wrapper of either.
/// A string that is not a canonical URI is rejected instead of being read as
/// a local path.
pub(crate) fn parse_artifact_source(value: &Value, origin: SourceOrigin<'_>) -> Result<FileSource> {
    parse_source(value, origin, false)
}

fn parse_source(value: &Value, origin: SourceOrigin<'_>, allow_path: bool) -> Result<FileSource> {
    match value {
        Value::String(value) if allow_path => source_from_string(origin.text(value)),
        Value::String(value) => {
            let uri = origin.text(value);
            ArtifactRef::validate_uri(&uri)?;
            Ok(FileSource::artifact_uri(uri))
        }
        Value::Object(object) => {
            if let Some(artifact) = object.get("artifact") {
                if object.contains_key("artifact_uri") || object.len() != 1 {
                    anyhow::bail!("ambiguous artifact wrapper contains other source fields");
                }
                return parse_source(artifact, origin, allow_path);
            }
            let artifact = ArtifactRef::from_value(value)
                .map_err(|error| anyhow::anyhow!("invalid artifact descriptor: {error}"))?;
            Ok(FileSource::artifact(artifact))
        }
        _ if allow_path => anyhow::bail!("expected a string or artifact descriptor object"),
        _ => anyhow::bail!("expected an artifact URI or artifact descriptor object"),
    }
}

/// Classify a fully resolved string as a path or artifact URI. It performs no
/// interpolation, so a resolved value cannot be expanded a second time.
fn source_from_string(value: String) -> Result<FileSource> {
    if value.starts_with("artifact://") {
        ArtifactRef::validate_uri(&value)?;
        Ok(FileSource::artifact_uri(value))
    } else {
        Ok(FileSource::path(value))
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

    fn path_of(source: FileSource) -> String {
        match source {
            FileSource::Path(path) => path.to_string_lossy().into_owned(),
            other => panic!("expected a path source, got {other:?}"),
        }
    }

    #[test]
    fn a_configured_path_is_interpolated_exactly_once() {
        let context = Context::from([
            ("upload".to_owned(), serde_json::json!("/in/${ctx.token}")),
            ("token".to_owned(), serde_json::json!("secret")),
        ]);
        let resolve = |config: Value| path_of(get_file_source(&config, &context, "test").unwrap());

        assert_eq!(
            resolve(serde_json::json!({"path": "${ctx.upload}"})),
            "/in/${ctx.token}"
        );
        assert_eq!(
            resolve(serde_json::json!({"source_key": "upload"})),
            "/in/${ctx.token}"
        );
        assert_eq!(
            resolve(serde_json::json!({"path": "/in/\\${ctx.token}"})),
            "/in/${ctx.token}"
        );
        assert_eq!(
            resolve(serde_json::json!({"path": "/in/${ctx.token}"})),
            "/in/secret"
        );
    }

    #[test]
    fn artifact_only_sources_reject_paths_from_either_origin() {
        let context = Context::from([("file".to_owned(), serde_json::json!("/etc/hosts"))]);
        for origin in [SourceOrigin::Data, SourceOrigin::Config(&context)] {
            for value in [
                serde_json::json!("/etc/hosts"),
                serde_json::json!("${ctx.file}"),
                serde_json::json!({"artifact": "/etc/hosts"}),
                serde_json::json!(7),
            ] {
                assert!(parse_artifact_source(&value, origin).is_err(), "{value}");
            }
            let uri = serde_json::json!(format!("artifact://sha256/{}", "a".repeat(64)));
            assert!(matches!(
                parse_artifact_source(&uri, origin).unwrap(),
                FileSource::ArtifactUri(_)
            ));
        }
    }
}
