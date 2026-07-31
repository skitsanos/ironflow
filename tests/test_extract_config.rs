//! Cross-format configuration validation for extraction nodes.

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;

async fn error(node_name: &str, config: serde_json::Value) -> String {
    NodeRegistry::with_builtins()
        .get(node_name)
        .unwrap()
        .execute(&config, &Context::new())
        .await
        .unwrap_err()
        .to_string()
}

#[tokio::test]
async fn extractors_reject_present_invalid_configuration_before_io() {
    let nodes = [
        "extract_html",
        "extract_pdf",
        "extract_word",
        "extract_pptx",
        "extract_srt",
        "extract_vtt",
        "extract_xlsx",
    ];
    for node in nodes {
        let message = error(node, serde_json::json!({ "path": false })).await;
        assert!(
            message.contains("'path' must be a string"),
            "{node}: {message}"
        );

        let message = error(
            node,
            serde_json::json!({ "path": "/does/not/exist", "output_key": 7 }),
        )
        .await;
        assert!(
            message.contains("'output_key' must be a string"),
            "{node}: {message}"
        );
    }

    for node in &nodes[..6] {
        let message = error(
            node,
            serde_json::json!({ "path": "/does/not/exist", "format": [] }),
        )
        .await;
        assert!(
            message.contains("'format' must be a string"),
            "{node}: {message}"
        );
    }

    for node in [
        "extract_html",
        "extract_pdf",
        "extract_word",
        "extract_pptx",
    ] {
        let message = error(
            node,
            serde_json::json!({ "path": "/does/not/exist", "metadata_key": {} }),
        )
        .await;
        assert!(
            message.contains("'metadata_key' must be a string"),
            "{node}: {message}"
        );
    }

    for node in ["extract_srt", "extract_vtt"] {
        let message = error(
            node,
            serde_json::json!({ "path": "/does/not/exist", "cues_key": null }),
        )
        .await;
        assert!(
            message.contains("'cues_key' must be a string"),
            "{node}: {message}"
        );
    }

    let message = error(
        "extract_xlsx",
        serde_json::json!({ "path": "/does/not/exist", "has_header": "maybe" }),
    )
    .await;
    assert!(
        message.contains("'has_header' must be a boolean"),
        "{message}"
    );

    let message = error(
        "extract_pptx",
        serde_json::json!({ "path": "/does/not/exist", "include_image_bytes": 1 }),
    )
    .await;
    assert!(
        message.contains("'include_image_bytes' must be a boolean"),
        "{message}"
    );

    let message = error(
        "extract_pptx",
        serde_json::json!({ "path": "/does/not/exist", "media_mode": [] }),
    )
    .await;
    assert!(
        message.contains("'media_mode' must be a string"),
        "{message}"
    );
}
