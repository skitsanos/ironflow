#[path = "notification_http/support.rs"]
mod support;

#[path = "notification_http/examples.rs"]
mod examples;
#[path = "notification_http/execution.rs"]
mod execution;

use serde_json::{Value, json};
use std::time::Duration;
use support::{NODES, Server, assert_redacted, config, execute, isolated};

#[tokio::test]
async fn declared_and_chunked_responses_obey_the_http_limit() {
    if isolated("declared_and_chunked_responses_obey_the_http_limit").await {
        return;
    }
    for node in NODES {
        for status in [200, 400] {
            for declared in [Some(4096), None] {
                let server = Server::start(
                    status,
                    "text/plain",
                    vec![vec![b'x'; 4096]],
                    declared,
                    false,
                    false,
                )
                .await;
                let result = execute(node, &config(node, &server.url)).await;
                assert!(
                    result.is_err(),
                    "{node}: oversized response published output"
                );
                let error = result.unwrap_err();
                assert!(
                    error.to_string().contains("IRONFLOW_MAX_HTTP_BODY_BYTES"),
                    "{node}: {error}"
                );
                assert_redacted(&error);
            }
        }
    }
}

#[tokio::test]
async fn oversized_headers_fail_without_waiting_for_body_bytes() {
    if isolated("oversized_headers_fail_without_waiting_for_body_bytes").await {
        return;
    }
    for node in NODES {
        let server = Server::start(200, "text/plain", vec![], Some(4096), true, false).await;
        let error = tokio::time::timeout(
            Duration::from_secs(2),
            execute(node, &config(node, &server.url)),
        )
        .await
        .expect("waited for an oversized declared body")
        .unwrap_err();
        assert!(
            error.to_string().contains("IRONFLOW_MAX_HTTP_BODY_BYTES"),
            "{node}: {error}"
        );
        assert_redacted(&error);
    }
}

#[tokio::test]
async fn exact_limit_and_complete_outputs_are_preserved() {
    if isolated("exact_limit_and_complete_outputs_are_preserved").await {
        return;
    }
    for node in NODES {
        for length in [0, 1023, 1024, 1025] {
            for declared in [Some(length as u64), None] {
                let bytes = vec![b'x'; length];
                let chunks = bytes.chunks(256).map(<[u8]>::to_vec).collect();
                let server = Server::start(200, "text/plain", chunks, declared, false, false).await;
                let result = execute(node, &config(node, &server.url)).await;
                if length > 1024 {
                    let error = result.unwrap_err();
                    assert!(
                        error.to_string().contains("IRONFLOW_MAX_HTTP_BODY_BYTES"),
                        "{node}: {error}"
                    );
                } else {
                    let output = result.unwrap();
                    assert_eq!(output["result_status"], 200);
                    assert_eq!(output["result_success"], true);
                    assert_eq!(output["result_data"], String::from_utf8(bytes).unwrap());
                }
            }
        }
        let body = json!({"id": "fixture", "accepted": true, "items": [1, 2]});
        let server = Server::start(
            201,
            "application/json",
            vec![body.to_string().into_bytes()],
            None,
            false,
            false,
        )
        .await;
        let output = execute(node, &config(node, &server.url)).await.unwrap();
        assert_eq!(output["result_status"], 201);
        assert_eq!(output["result_data"], body);
    }
}

#[tokio::test]
async fn notification_text_decoding_retains_charset_bom_and_lossy_utf8() {
    if isolated("notification_text_decoding_retains_charset_bom_and_lossy_utf8").await {
        return;
    }
    for node in &NODES[1..] {
        for (content_type, bytes, expected) in [
            (
                "text/plain; charset=windows-1252",
                vec![b'c', b'a', b'f', 0xe9],
                "caf\u{e9}",
            ),
            (
                "text/plain; charset=utf-8",
                vec![0xef, 0xbb, 0xbf, b'o', b'k'],
                "ok",
            ),
            ("text/plain", vec![b'a', 0xff], "a\u{fffd}"),
        ] {
            let server = Server::start(200, content_type, vec![bytes], None, false, false).await;
            let output = execute(node, &config(node, &server.url)).await.unwrap();
            assert_eq!(output["result_data"], expected);
        }
    }
}

#[tokio::test]
async fn provider_errors_and_interrupted_streams_never_publish_partial_success() {
    if isolated("provider_errors_and_interrupted_streams_never_publish_partial_success").await {
        return;
    }
    for node in &NODES[1..] {
        for status in [400, 429, 500] {
            let body = b"password=body-sentinel invalid";
            for declared in [Some(body.len() as u64), None] {
                let server = Server::start(
                    status,
                    "text/plain",
                    vec![body.to_vec()],
                    declared,
                    false,
                    false,
                )
                .await;
                let error = execute(node, &config(node, &server.url)).await.unwrap_err();
                assert!(
                    error.to_string().contains(&format!("status {status}")),
                    "{node}: {error}"
                );
                assert!(error.to_string().contains("invalid"), "{node}: {error}");
                assert_redacted(&error);
            }
        }
        // Even a complete JSON prefix is not a complete HTTP message on failure.
        for (declared, stall, broken) in [
            (Some(100), false, true),
            (None, false, true),
            (None, true, false),
        ] {
            let server = Server::start(
                200,
                "application/json",
                vec![b"{\"ok\":true}".to_vec()],
                declared,
                stall,
                broken,
            )
            .await;
            let mut params = config(node, &server.url);
            params["timeout"] = json!(0.3);
            let error = execute(node, &params).await.unwrap_err();
            assert!(
                error.to_string().contains("Failed to read"),
                "{node}: {error}"
            );
            assert_redacted(&error);
            assert!(
                error
                    .downcast_ref::<ironflow::nodes::NodeFailure>()
                    .is_none()
            );
        }
    }
}

fn assert_no_output(context: &Value, prefix: &str) {
    for suffix in ["status", "data", "success"] {
        assert!(
            context.get(format!("{prefix}_{suffix}")).is_none(),
            "{context}"
        );
    }
}
