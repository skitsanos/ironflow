use std::path::Path;

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;
use serde_json::{Value, json};

#[path = "support/cache.rs"]
mod fixture;

fn assert_miss(output: &Context) {
    assert_eq!(output["cache_hit"], false);
    assert_eq!(output["cached_value"], Value::Null);
}

async fn set(directory: &Path, key: &str, value: Value, ttl: Option<u64>) -> Context {
    let mut config = json!({"backend": "file", "cache_dir": directory, "key": key, "value": value});
    if let Some(ttl) = ttl {
        config["ttl"] = json!(ttl);
    }
    NodeRegistry::with_builtins()
        .get("cache_set")
        .unwrap()
        .execute(&config, &Context::new())
        .await
        .unwrap()
}

async fn get(directory: &Path, key: &str) -> anyhow::Result<Context> {
    NodeRegistry::with_builtins()
        .get("cache_get")
        .unwrap()
        .execute(
            &json!({"backend": "file", "cache_dir": directory, "key": key}),
            &Context::new(),
        )
        .await
}

#[tokio::test]
async fn punctuation_keys_do_not_replace_each_other() {
    let directory = tempfile::tempdir().unwrap();
    set(directory.path(), "report:a/b", json!("slash value"), None).await;
    set(
        directory.path(),
        "report:a?b",
        json!("question value"),
        None,
    )
    .await;
    let slash = get(directory.path(), "report:a/b").await.unwrap();
    assert_eq!(slash["cache_hit"], true);
    assert_eq!(slash["cached_value"], "slash value");
    let question = get(directory.path(), "report:a?b").await.unwrap();
    assert_eq!(question["cache_hit"], true);
    assert_eq!(question["cached_value"], "question value");
}

#[tokio::test]
async fn file_keys_preserve_exact_unicode_case_and_path_identity() {
    let directory = tempfile::tempdir().unwrap();
    let long_key = "long:key/".repeat(256);
    let keys = [
        "report:a/b",
        "report:a?b",
        "report_a_b",
        "Case",
        "case",
        "caf\u{e9}",
        "cafe\u{301}",
        "\u{6587}/\u{6863}",
        "\u{6587}?\u{6863}",
        "../../outside",
        "/",
        "\\",
        ".",
        "..",
        "",
        "\0",
        "a%2Fb",
        "a/b",
        &long_key,
    ];
    for (index, key) in keys.iter().enumerate() {
        let output = set(directory.path(), key, json!({"index": index}), None).await;
        assert_eq!(output["cache_key"], *key);
        assert_eq!(output["cache_stored"], true);
    }
    for (index, key) in keys.iter().enumerate() {
        let output = get(directory.path(), key).await.unwrap();
        assert_eq!(output["cache_hit"], true, "{key:?}");
        assert_eq!(output["cached_value"], json!({"index": index}), "{key:?}");
        let path = fixture::entry_path(directory.path(), key);
        assert_eq!(path.file_name().unwrap().len(), 69);
        let record: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(record["schema_version"], 1);
        assert_eq!(record["key"], *key);
        assert!(record.get("expires_at").is_none());
    }
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
    assert_eq!(
        std::fs::read_dir(directory.path().join("v1"))
            .unwrap()
            .count(),
        keys.len()
    );
}

#[tokio::test]
async fn digest_layout_matches_sha256_of_utf8_key_bytes() {
    let directory = tempfile::tempdir().unwrap();
    set(directory.path(), "abc", json!(true), None).await;
    let path = directory
        .path()
        .join("v1/ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad.json");
    assert!(path.is_file());
}

#[tokio::test]
async fn identity_is_checked_before_hits_and_expiry_cleanup() {
    let directory = tempfile::tempdir().unwrap();
    set(directory.path(), "", json!("valid"), None).await;
    let path = fixture::entry_path(directory.path(), "");
    for record in [
        json!({"schema_version": 1, "key": "wrong", "value": "wrong", "expires_at": 0}),
        json!({"schema_version": 1, "value": "missing key", "expires_at": 0}),
        json!({"schema_version": 1, "key": null, "value": "null key", "expires_at": 0}),
        json!({"key": "", "value": "missing version", "expires_at": 0}),
        json!({"schema_version": 2, "key": "", "value": "future version", "expires_at": 0}),
    ] {
        let bytes = serde_json::to_vec(&record).unwrap();
        std::fs::write(&path, &bytes).unwrap();
        assert_miss(&get(directory.path(), "").await.unwrap());
        assert_eq!(
            std::fs::read(&path).unwrap(),
            bytes,
            "unverified entry must not be deleted"
        );
    }
}

#[tokio::test]
async fn a_valid_record_copied_under_another_digest_is_not_a_hit() {
    let directory = tempfile::tempdir().unwrap();
    set(directory.path(), "first", json!("first value"), None).await;
    let first = fixture::entry_path(directory.path(), "first");
    let second = fixture::entry_path(directory.path(), "second");
    std::fs::copy(&first, &second).unwrap();
    assert_miss(&get(directory.path(), "second").await.unwrap());
    assert_eq!(
        get(directory.path(), "first").await.unwrap()["cached_value"],
        "first value"
    );
    assert_eq!(
        std::fs::read(first).unwrap(),
        std::fs::read(second).unwrap()
    );
}

#[tokio::test]
async fn legacy_files_are_misses_and_are_neither_migrated_nor_removed() {
    let directory = tempfile::tempdir().unwrap();
    let bytes = br#"{"value":"ambiguous legacy value","expires_at":0}"#;
    for name in ["report_a_b.json", "plain.json"] {
        std::fs::write(directory.path().join(name), bytes).unwrap();
    }
    for key in ["report:a/b", "report:a?b", "plain"] {
        assert_miss(&get(directory.path(), key).await.unwrap());
    }
    assert!(!directory.path().join("v1").exists());
    set(directory.path(), "report:a/b", json!("fresh"), None).await;
    assert_eq!(
        get(directory.path(), "report:a/b").await.unwrap()["cached_value"],
        "fresh"
    );
    assert_miss(&get(directory.path(), "report:a?b").await.unwrap());
    for name in ["report_a_b.json", "plain.json"] {
        assert_eq!(std::fs::read(directory.path().join(name)).unwrap(), bytes);
    }
}

#[tokio::test]
async fn expiry_and_overwrite_only_affect_the_matching_key() {
    let directory = tempfile::tempdir().unwrap();
    set(directory.path(), "a/b", json!("expired"), Some(0)).await;
    set(directory.path(), "a?b", json!("retained"), None).await;
    assert_miss(&get(directory.path(), "a/b").await.unwrap());
    assert!(!fixture::entry_path(directory.path(), "a/b").exists());
    assert_eq!(
        get(directory.path(), "a?b").await.unwrap()["cached_value"],
        "retained"
    );
    set(directory.path(), "a/b", Value::Null, Some(3600)).await;
    let null = get(directory.path(), "a/b").await.unwrap();
    assert_eq!(null["cache_hit"], true);
    assert_eq!(null["cached_value"], Value::Null);
    set(directory.path(), "a/b", json!("replacement"), None).await;
    assert_eq!(
        get(directory.path(), "a/b").await.unwrap()["cached_value"],
        "replacement"
    );
    assert_eq!(
        std::fs::read_dir(directory.path().join("v1"))
            .unwrap()
            .count(),
        2
    );
}

#[tokio::test]
async fn malformed_json_remains_an_error_and_is_not_deleted() {
    let directory = tempfile::tempdir().unwrap();
    set(directory.path(), "corrupt", json!(1), None).await;
    let path = fixture::entry_path(directory.path(), "corrupt");
    for bytes in [
        b"{".as_slice(),
        br#"{"schema_version":"1","key":"corrupt","value":1}"#,
    ] {
        std::fs::write(&path, bytes).unwrap();
        let error = get(directory.path(), "corrupt")
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("Corrupt cache file"), "{error}");
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
    }
}

#[tokio::test]
async fn memory_backend_still_distinguishes_keys_and_handles_ttl() {
    let registry = NodeRegistry::with_builtins();
    let prefix = uuid::Uuid::new_v4();
    for (index, suffix) in ["a/b", "a?b", "caf\u{e9}", "cafe\u{301}"]
        .iter()
        .enumerate()
    {
        let key = format!("{prefix}:{suffix}");
        registry
            .get("cache_set")
            .unwrap()
            .execute(&json!({"key": key, "value": index}), &Context::new())
            .await
            .unwrap();
    }
    for (index, suffix) in ["a/b", "a?b", "caf\u{e9}", "cafe\u{301}"]
        .iter()
        .enumerate()
    {
        let key = format!("{prefix}:{suffix}");
        let result = registry
            .get("cache_get")
            .unwrap()
            .execute(&json!({"key": key}), &Context::new())
            .await
            .unwrap();
        assert_eq!(result["cache_hit"], true);
        assert_eq!(result["cached_value"], index);
    }
    let key = format!("{prefix}:a/b");
    registry
        .get("cache_set")
        .unwrap()
        .execute(&json!({"key": key, "value": 99, "ttl": 0}), &Context::new())
        .await
        .unwrap();
    assert_miss(
        &registry
            .get("cache_get")
            .unwrap()
            .execute(&json!({"key": key}), &Context::new())
            .await
            .unwrap(),
    );
}

#[tokio::test]
async fn an_expired_deadline_prevents_file_cache_io() {
    use ironflow::util::execution::with_execution_deadline;
    let directory = tempfile::tempdir().unwrap();
    let cache_dir = directory.path().join("cache");
    for node in ["cache_set", "cache_get"] {
        let config =
            json!({"backend": "file", "cache_dir": cache_dir, "key": "deadline", "value": 1});
        let registry = NodeRegistry::with_builtins();
        let handler = registry.get(node).unwrap();
        let ctx = Context::new();
        let error = with_execution_deadline(
            Some(tokio::time::Instant::now()),
            handler.execute(&config, &ctx),
        )
        .await
        .unwrap_err();
        assert!(
            error.to_string().contains("step deadline exceeded"),
            "{error:#}"
        );
        assert!(!cache_dir.exists());
    }
}
