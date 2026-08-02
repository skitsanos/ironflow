use std::path::Path;

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;

static ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

struct Environment {
    file_limit: Option<std::ffi::OsString>,
    artifact_dir: Option<std::ffi::OsString>,
}

impl Environment {
    fn set(limit: u64, artifacts: &Path) -> Self {
        let environment = Self {
            file_limit: std::env::var_os("IRONFLOW_MAX_FILE_BYTES"),
            artifact_dir: std::env::var_os("IRONFLOW_ARTIFACT_DIR"),
        };
        // SAFETY: every environment mutation in this test binary is serialized.
        unsafe {
            std::env::set_var("IRONFLOW_MAX_FILE_BYTES", limit.to_string());
            std::env::set_var("IRONFLOW_ARTIFACT_DIR", artifacts);
        }
        environment
    }
}

impl Drop for Environment {
    fn drop(&mut self) {
        // SAFETY: the environment lock is held until this guard drops.
        unsafe {
            restore("IRONFLOW_MAX_FILE_BYTES", self.file_limit.take());
            restore("IRONFLOW_ARTIFACT_DIR", self.artifact_dir.take());
        }
    }
}

unsafe fn restore(key: &str, value: Option<std::ffi::OsString>) {
    match value {
        Some(value) => unsafe { std::env::set_var(key, value) },
        None => unsafe { std::env::remove_var(key) },
    }
}

#[tokio::test]
async fn malformed_base64_keeps_existing_destination_and_removes_staging() {
    let _lock = ENV_LOCK.lock().await;
    let directory = tempfile::tempdir().unwrap();
    let _environment = Environment::set(100, &directory.path().join("artifacts"));
    let destination = directory.path().join("output.bin");
    std::fs::write(&destination, b"original").unwrap();
    let node = NodeRegistry::with_builtins().get("write_file").unwrap();
    let error = node
        .execute(
            &serde_json::json!({
                "path": destination,
                "content": "!!!!",
                "encoding": "base64"
            }),
            &Context::new(),
        )
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("invalid base64"), "{error}");
    assert_eq!(std::fs::read(&destination).unwrap(), b"original");
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
}

#[tokio::test]
async fn append_limit_applies_to_final_file_size() {
    let _lock = ENV_LOCK.lock().await;
    let directory = tempfile::tempdir().unwrap();
    let _environment = Environment::set(8, &directory.path().join("artifacts"));
    let destination = directory.path().join("output.txt");
    std::fs::write(&destination, b"1234567").unwrap();
    let node = NodeRegistry::with_builtins().get("write_file").unwrap();
    let error = node
        .execute(
            &serde_json::json!({"path": destination, "content": "89", "append": true}),
            &Context::new(),
        )
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("IRONFLOW_MAX_FILE_BYTES"), "{error}");
    assert_eq!(std::fs::read(&destination).unwrap(), b"1234567");
}

#[tokio::test]
async fn artifact_source_streams_to_destination() {
    let _lock = ENV_LOCK.lock().await;
    let directory = tempfile::tempdir().unwrap();
    let _environment = Environment::set(100, &directory.path().join("artifacts"));
    let source = directory.path().join("source.bin");
    let destination = directory.path().join("output.bin");
    std::fs::write(&source, b"artifact payload").unwrap();
    let registry = NodeRegistry::with_builtins();
    let read = registry
        .get("read_file")
        .unwrap()
        .execute(
            &serde_json::json!({"path": source, "encoding": "artifact"}),
            &Context::new(),
        )
        .await
        .unwrap();
    let context = Context::from([("artifact".to_owned(), read["file_artifact"].clone())]);
    registry
        .get("write_file")
        .unwrap()
        .execute(
            &serde_json::json!({"path": destination, "source_key": "artifact"}),
            &context,
        )
        .await
        .unwrap();
    assert_eq!(std::fs::read(destination).unwrap(), b"artifact payload");
}

#[cfg(unix)]
#[tokio::test]
async fn destination_symlink_is_rejected_without_touching_target() {
    use std::os::unix::fs::symlink;

    let _lock = ENV_LOCK.lock().await;
    let directory = tempfile::tempdir().unwrap();
    let _environment = Environment::set(100, &directory.path().join("artifacts"));
    let target = directory.path().join("target.txt");
    let destination = directory.path().join("output.txt");
    std::fs::write(&target, b"target").unwrap();
    symlink(&target, &destination).unwrap();
    let error = NodeRegistry::with_builtins()
        .get("write_file")
        .unwrap()
        .execute(
            &serde_json::json!({"path": destination, "content": "hostile"}),
            &Context::new(),
        )
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("symlink"), "{error}");
    assert_eq!(std::fs::read(target).unwrap(), b"target");
}
