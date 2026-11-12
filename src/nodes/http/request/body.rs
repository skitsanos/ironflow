use anyhow::Result;
use serde_json::Value;

use crate::artifacts::FileSource;
use crate::engine::types::Context;
use crate::lua::interpolate::interpolate_ctx;
use crate::nodes::http::helpers::{body_value_to_text, build_form_body, interpolate_json_value};
use crate::util::execution::run_tracked_blocking_step;
use crate::util::file_source::{SourceOrigin, parse_artifact_source};

use self::multipart::MultipartField;

mod multipart;

#[derive(Clone, Debug)]
pub(super) enum RequestBody {
    None,
    Json(Value),
    Form(String),
    Text(String),
    Artifact(ArtifactUpload),
    Multipart(Vec<MultipartField>),
}

#[derive(Clone, Debug)]
pub(super) struct ArtifactUpload {
    source: FileSource,
    mime_type: Option<String>,
}

impl RequestBody {
    pub(super) fn resolve(config: &Value, ctx: &Context) -> Result<Self> {
        let body_type = match config.get("body_type") {
            None => "json",
            Some(Value::String(value)) => value.as_str(),
            Some(_) => anyhow::bail!("HTTP body_type must be a string"),
        };
        match body_type {
            "json" | "form" | "text" => resolve_inline_body(body_type, config, ctx),
            "artifact" => resolve_artifact_body(config, ctx),
            "multipart" => multipart::resolve(config, ctx),
            other => anyhow::bail!(
                "Unsupported body_type '{other}'. Expected one of: json, form, text, artifact, multipart"
            ),
        }
    }

    pub(super) fn has_payload(&self) -> bool {
        !matches!(self, Self::None)
    }

    pub(super) fn manages_framing(&self) -> bool {
        matches!(self, Self::Artifact(_) | Self::Multipart(_))
    }

    pub(super) async fn apply(
        &self,
        mut request: reqwest::RequestBuilder,
        has_content_type: bool,
    ) -> Result<reqwest::RequestBuilder> {
        match self {
            Self::None => {}
            Self::Json(body) => request = request.json(body),
            Self::Form(body) => {
                if !has_content_type {
                    request = request.header(
                        reqwest::header::CONTENT_TYPE,
                        "application/x-www-form-urlencoded",
                    );
                }
                request = request.body(body.clone());
            }
            Self::Text(body) => {
                if !has_content_type {
                    request =
                        request.header(reqwest::header::CONTENT_TYPE, "text/plain; charset=utf-8");
                }
                request = request.body(body.clone());
            }
            Self::Artifact(upload) => {
                let opened = open_artifact(upload).await?;
                if !has_content_type {
                    request = request.header(
                        reqwest::header::CONTENT_TYPE,
                        upload
                            .mime_type
                            .as_deref()
                            .unwrap_or("application/octet-stream"),
                    );
                }
                request = request
                    .header(reqwest::header::CONTENT_LENGTH, opened.size)
                    .body(reqwest::Body::from(tokio::fs::File::from_std(opened.file)));
            }
            Self::Multipart(fields) => {
                if has_content_type {
                    anyhow::bail!(
                        "HTTP multipart requests generate their own Content-Type boundary; remove the configured Content-Type header"
                    );
                }
                request = request.multipart(multipart::build(fields).await?);
            }
        }
        Ok(request)
    }
}

fn resolve_inline_body(body_type: &str, config: &Value, ctx: &Context) -> Result<RequestBody> {
    if config.get("body_key").is_some() || config.get("parts").is_some() {
        anyhow::bail!("HTTP body_key and parts require body_type='artifact' or 'multipart'");
    }
    let Some(body) = config.get("body") else {
        return Ok(RequestBody::None);
    };
    let body = interpolate_json_value(body, ctx);
    match body_type {
        "json" => Ok(RequestBody::Json(body)),
        "form" => Ok(RequestBody::Form(build_form_body(&body)?)),
        "text" => Ok(RequestBody::Text(body_value_to_text(&body))),
        _ => unreachable!("caller validates inline body type"),
    }
}

#[allow(clippy::expect_used)] // Infallible: exclusive source validation ensures body exists.
fn resolve_artifact_body(config: &Value, ctx: &Context) -> Result<RequestBody> {
    if config.get("parts").is_some() {
        anyhow::bail!("HTTP parts require body_type='multipart'");
    }
    let direct = config.get("body");
    let source_key = optional_string(config, "body_key")?;
    if direct.is_some() == source_key.is_some() {
        anyhow::bail!("HTTP body_type='artifact' requires exactly one of 'body' or 'body_key'");
    }
    let source = match source_key {
        Some(key) => ctx
            .get(&interpolate_ctx(key, ctx))
            .ok_or_else(|| anyhow::anyhow!("HTTP body_key '{key}' not found in context"))?,
        None => direct.expect("exclusive source validation ensures body exists"),
    };
    Ok(RequestBody::Artifact(artifact_upload(source)?))
}

/// An upload names stored bytes by descriptor or canonical URI, parsed by the
/// same artifact-only parser as `write_file`. The value is used as written:
/// neither a configured `body` nor a context value is interpolated.
fn artifact_upload(value: &Value) -> Result<ArtifactUpload> {
    let source = parse_artifact_source(value, SourceOrigin::Data).map_err(|error| {
        anyhow::anyhow!(
            "HTTP artifact source must be an artifact URI or descriptor object: {error}"
        )
    })?;
    let mime_type = match &source {
        FileSource::Artifact(artifact) => artifact.mime_type.clone(),
        FileSource::ArtifactUri(_) | FileSource::Path(_) => None,
    };
    Ok(ArtifactUpload { source, mime_type })
}

struct OpenedUpload {
    file: std::fs::File,
    size: u64,
}

async fn open_artifact(upload: &ArtifactUpload) -> Result<OpenedUpload> {
    let source = upload.source.clone();
    run_tracked_blocking_step(move |execution| {
        let (file, label) = source.open("HTTP artifact upload", &execution)?.into_parts();
        let size = file.metadata()?.len();
        let maximum = crate::util::limits::max_http_body_bytes();
        if size > maximum {
            anyhow::bail!(
                "HTTP artifact upload '{label}' is {size} bytes, exceeds IRONFLOW_MAX_HTTP_BODY_BYTES ({maximum})"
            );
        }
        Ok(OpenedUpload { file, size })
    })
    .await
}

fn optional_string<'a>(config: &'a Value, key: &str) -> Result<Option<&'a str>> {
    match config.get(key) {
        None => Ok(None),
        Some(Value::String(value)) if !value.is_empty() => Ok(Some(value)),
        Some(_) => anyhow::bail!("HTTP '{key}' must be a non-empty string"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn artifact_body_rejects_paths_and_ambiguous_sources() {
        let context = Context::from([("path".to_owned(), Value::String("/tmp/file".to_owned()))]);
        for config in [
            serde_json::json!({"body_type": "artifact", "body_key": "path"}),
            serde_json::json!({"body_type": "artifact", "body": {}, "body_key": "path"}),
        ] {
            assert!(RequestBody::resolve(&config, &context).is_err());
        }
    }

    #[test]
    fn artifact_sources_accept_only_uris_and_descriptors() {
        let digest = "a".repeat(64);
        let uri = format!("artifact://sha256/{digest}");
        let descriptor = serde_json::json!({
            "artifact_uri": uri, "sha256": digest, "size_bytes": 7, "mime_type": "image/png"
        });
        let context = Context::from([("uri".to_owned(), Value::String(uri.clone()))]);
        let upload = |source: Value| {
            let config = serde_json::json!({"body_type": "artifact", "body": source});
            match RequestBody::resolve(&config, &context) {
                Ok(RequestBody::Artifact(upload)) => Ok(upload),
                Ok(other) => panic!("unexpected body {other:?}"),
                Err(error) => Err(error.to_string()),
            }
        };

        for source in [
            descriptor.clone(),
            serde_json::json!({"artifact": descriptor.clone()}),
        ] {
            let upload = upload(source).unwrap();
            assert!(matches!(upload.source, FileSource::Artifact(_)));
            assert_eq!(upload.mime_type.as_deref(), Some("image/png"));
        }
        for source in [serde_json::json!(uri), serde_json::json!({"artifact": uri})] {
            let upload = upload(source).unwrap();
            assert!(matches!(upload.source, FileSource::ArtifactUri(_)));
            assert_eq!(upload.mime_type, None);
        }

        let mut extra = descriptor.clone();
        extra["artifact"] = serde_json::json!(uri);
        for source in [
            serde_json::json!("/tmp/file"),
            // A body URI is data: it is not interpolated into a valid one.
            serde_json::json!("${ctx.uri}"),
            serde_json::json!({"artifact": "/tmp/file"}),
            serde_json::json!({"artifact": uri, "mime_type": "image/png"}),
            extra,
            serde_json::json!({}),
            serde_json::json!(7),
        ] {
            let error = upload(source.clone()).unwrap_err();
            assert!(error.starts_with("HTTP "), "{source}: {error}");
        }
    }

    #[test]
    fn multipart_contract_is_strict() {
        let context = Context::new();
        for config in [
            serde_json::json!({"body_type": "multipart", "parts": []}),
            serde_json::json!({"body_type": "multipart", "parts": [{"name": "x"}]}),
            serde_json::json!({"body_type": "multipart", "parts": [{"name": "x", "text": "y", "filename": "z"}]}),
            serde_json::json!({"body_type": "multipart", "parts": [{"name": "x", "text": "y", "unknown": true}]}),
        ] {
            assert!(RequestBody::resolve(&config, &context).is_err());
        }
    }
}
