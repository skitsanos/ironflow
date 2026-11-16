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
fn configured_artifact_uris_interpolate_once_and_context_values_are_data() {
    let uri = format!("artifact://sha256/{}", "a".repeat(64));
    let context = Context::from([
        ("uri".to_owned(), Value::String(uri.clone())),
        (
            "template".to_owned(),
            Value::String("${ctx.uri}".to_owned()),
        ),
        ("path".to_owned(), Value::String("/tmp/file".to_owned())),
    ]);
    let resolved_uri = |config: Value| -> Result<FileSource> {
        match RequestBody::resolve(&config, &context)? {
            RequestBody::Artifact(upload) => Ok(upload.source),
            RequestBody::Multipart(fields) => match fields.into_iter().next() {
                Some(MultipartField::Artifact { upload, .. }) => Ok(upload.source),
                other => panic!("unexpected part {other:?}"),
            },
            other => panic!("unexpected body {other:?}"),
        }
    };
    let part = |part: Value| serde_json::json!({"body_type": "multipart", "parts": [part]});

    for config in [
        serde_json::json!({"body_type": "artifact", "body": "${ctx.uri}"}),
        serde_json::json!({"body_type": "artifact", "body": {"artifact": "${ctx.uri}"}}),
        serde_json::json!({"body_type": "artifact", "body_key": "uri"}),
        part(serde_json::json!({"name": "file", "artifact": "${ctx.uri}"})),
        part(serde_json::json!({"name": "file", "source_key": "uri"})),
    ] {
        let source =
            resolved_uri(config.clone()).unwrap_or_else(|error| panic!("{config}: {error}"));
        assert!(
            matches!(&source, FileSource::ArtifactUri(resolved) if *resolved == uri),
            "{config}: {source:?}"
        );
    }

    // One pass only, a context value is never a template, and a
    // placeholder that resolves to a path is still not an artifact.
    for config in [
        serde_json::json!({"body_type": "artifact", "body": "${ctx.template}"}),
        serde_json::json!({"body_type": "artifact", "body": "\\${ctx.uri}"}),
        serde_json::json!({"body_type": "artifact", "body": "${ctx.path}"}),
        serde_json::json!({"body_type": "artifact", "body_key": "template"}),
        part(serde_json::json!({"name": "file", "artifact": "${ctx.path}"})),
        part(serde_json::json!({"name": "file", "source_key": "template"})),
    ] {
        assert!(resolved_uri(config.clone()).is_err(), "{config}");
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
