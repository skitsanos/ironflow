#[path = "schema_refs/support.rs"]
mod support;

use ironflow::nodes::NodeFailure;
use serde_json::{Value, json};
use support::{SchemaServer, assert_reference_error, execute};

#[tokio::test]
async fn remote_references_return_errors_without_panicking_or_fetching() {
    let server = SchemaServer::start().await;
    for node in ["validate_schema", "json_validate"] {
        for source in ["inline", "object", "string"] {
            for path in ["/schema", "/slow"] {
                let schema = json!({"$ref": format!("{}{path}", server.url)});
                let error = execute(node, schema, json!(1), source).await.unwrap_err();
                assert_reference_error(&error);
            }
        }
    }
    server.assert_unused();
}

#[tokio::test]
async fn all_external_resolution_paths_fail_without_uri_credential_disclosure() {
    let server = SchemaServer::start().await;
    let host = server.url.strip_prefix("http://").unwrap();
    let private_uri = format!(
        "http://uri-user-sentinel:uri-password-sentinel@{host}/schema?token=uri-query-sentinel"
    );
    let schemas = [
        json!({"$ref": private_uri}),
        json!({"type": "object", "properties": {"value": {"$ref": private_uri}}}),
        json!({"$id": format!("{}/root.json", server.url), "$ref": "relative.json"}),
        json!({"$schema": "https://json-schema.org/draft/2020-12/schema", "$dynamicRef": private_uri}),
        json!({"$schema": private_uri, "type": "integer"}),
        json!({"$ref": "https://127.0.0.1:1/unreachable.json"}),
        json!({"$ref": "urn:ironflow:missing-schema"}),
    ];
    for node in ["validate_schema", "json_validate"] {
        for schema in &schemas {
            let error = execute(node, schema.clone(), json!({"value": 1}), "inline")
                .await
                .unwrap_err();
            assert_reference_error(&error);
            let message = format!("{error:#}");
            for secret in [
                "uri-user-sentinel",
                "uri-password-sentinel",
                "uri-query-sentinel",
            ] {
                assert!(!message.contains(secret), "{message}");
            }
            assert!(error.source().is_none());
        }
    }
    server.assert_unused();
}

#[tokio::test]
async fn file_references_are_rejected_even_for_valid_readable_schema_files() {
    let directory = tempfile::tempdir().unwrap();
    let existing = directory.path().join("local.json");
    std::fs::write(&existing, r#"{"type":"integer"}"#).unwrap();
    for path in [existing, directory.path().join("missing.json")] {
        let reference = url::Url::from_file_path(path).unwrap().to_string();
        for node in ["validate_schema", "json_validate"] {
            let error = execute(node, json!({"$ref": reference}), json!(1), "inline")
                .await
                .unwrap_err();
            assert_reference_error(&error);
        }
    }
}

#[tokio::test]
async fn bundled_references_and_standard_drafts_validate_without_retrieval() {
    let server = SchemaServer::start().await;
    let schemas = [
        json!({"$defs": {"integer": {"type": "integer"}}, "$ref": "#/$defs/integer"}),
        json!({"$schema": "https://json-schema.org/draft/2020-12/schema", "$defs": {
            "integer": {"$anchor": "integer", "type": "integer"}}, "$ref": "#integer"}),
        json!({"$id": format!("{}/root.json", server.url), "$defs": {
            "integer": {"$id": "integer.json", "type": "integer"}}, "$ref": "integer.json"}),
        json!({"$id": format!("{}/root.json", server.url), "$defs": {
            "integer": {"$id": "integer.json", "type": "integer"}},
            "$ref": format!("{}/integer.json", server.url)}),
    ];
    for node in ["validate_schema", "json_validate"] {
        for source in ["inline", "object", "string"] {
            for schema in &schemas {
                let output = execute(node, schema.clone(), json!(7), source)
                    .await
                    .unwrap();
                assert_eq!(output["validation_success"], true);
                assert_eq!(output["validation_errors"], json!([]));
                let error = execute(node, schema.clone(), json!("not an integer"), source)
                    .await
                    .unwrap_err();
                let failure = error
                    .downcast_ref::<NodeFailure>()
                    .expect("data errors must retain structured output");
                assert_eq!(failure.output()["validation_success"], false);
            }
        }
        for draft in [
            "http://json-schema.org/draft-04/schema#",
            "http://json-schema.org/draft-06/schema#",
            "http://json-schema.org/draft-07/schema#",
            "https://json-schema.org/draft/2019-09/schema",
            "https://json-schema.org/draft/2020-12/schema",
        ] {
            execute(
                node,
                json!({"$schema": draft, "type": "integer"}),
                json!(1),
                "inline",
            )
            .await
            .unwrap();
        }
    }
    server.assert_unused();
}

#[tokio::test]
async fn schema_errors_and_boolean_schemas_keep_existing_failure_contracts() {
    for node in ["validate_schema", "json_validate"] {
        let output = execute(node, Value::Bool(true), json!(1), "inline")
            .await
            .unwrap();
        assert_eq!(output["validation_success"], true);
        let error = execute(node, Value::Bool(false), json!(1), "inline")
            .await
            .unwrap_err();
        assert!(error.downcast_ref::<NodeFailure>().is_some());
        let error = execute(node, json!({"type": 12}), json!(1), "inline")
            .await
            .unwrap_err();
        assert!(error.to_string().contains("Invalid JSON schema"));
        let error = execute(node, json!({"$ref": "#/$defs/missing"}), json!(1), "inline")
            .await
            .unwrap_err();
        assert_reference_error(&error);
    }
}

#[tokio::test]
async fn reference_shaped_data_and_annotations_are_not_treated_as_retrieval_requests() {
    let server = SchemaServer::start().await;
    let data = json!({"$ref": format!("{}/data-not-schema", server.url)});
    let schema = json!({"const": data, "examples": [data]});
    for node in ["validate_schema", "json_validate"] {
        execute(node, schema.clone(), data.clone(), "inline")
            .await
            .unwrap();
    }
    server.assert_unused();
}

#[tokio::test]
async fn reference_failures_terminalize_and_can_be_recovered_by_the_workflow() {
    use ironflow::engine::executor::WorkflowEngine;
    use ironflow::engine::types::{
        Context, FlowDefinition, RetryConfig, RunStatus, StepDefinition, TaskStatus,
    };
    use ironflow::nodes::NodeRegistry;
    use ironflow::storage::{StateStore, null_store::NullStateStore};
    use std::sync::Arc;
    use std::time::Duration;

    let server = SchemaServer::start().await;
    for node in ["validate_schema", "json_validate"] {
        for recover in [false, true] {
            let store = Arc::new(NullStateStore::new());
            let engine = WorkflowEngine::new(
                Arc::new(NodeRegistry::with_builtins()),
                store.clone(),
                Some(1),
            );
            let mut steps = vec![StepDefinition {
                name: "validate".into(),
                node_type: node.into(),
                config: json!({"source_key": "data", "schema": {"$ref": format!("{}/schema", server.url)}}),
                dependencies: vec![],
                retry: RetryConfig::default(),
                timeout_s: Some(5.0),
                route: None,
                on_error: recover.then(|| "recover".into()),
            }];
            if recover {
                steps.push(StepDefinition {
                    name: "recover".into(),
                    node_type: "code".into(),
                    config: json!({"source": "return { recovered = true }"}),
                    dependencies: vec![],
                    retry: RetryConfig::default(),
                    timeout_s: None,
                    route: None,
                    on_error: None,
                });
            }
            let flow = FlowDefinition {
                name: "schema-reference-error".into(),
                steps,
            };
            let handle = engine
                .start(&flow, Context::from([("data".into(), json!(1))]))
                .await
                .unwrap();
            let id = tokio::time::timeout(Duration::from_secs(10), handle.wait())
                .await
                .expect("schema reference left the workflow stalled")
                .unwrap();
            let info = store.get_run_info(&id).await.unwrap();
            assert_eq!(
                info.status,
                if recover {
                    RunStatus::Success
                } else {
                    RunStatus::Failed
                }
            );
            assert_eq!(info.tasks["validate"].status, TaskStatus::Failed);
            assert!(
                info.tasks["validate"]
                    .error
                    .as_deref()
                    .unwrap()
                    .contains("External schema retrieval is disabled")
            );
            assert!(!info.ctx.contains_key("validation_success"));
            if recover {
                assert_eq!(info.ctx["recovered"], true);
            }
        }
    }
    server.assert_unused();
}
