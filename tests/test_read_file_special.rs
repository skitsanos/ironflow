// IF-049: read_file's size guard trusted metadata().len(), which is 0 for
// special files like /dev/zero, so the read streamed unbounded. Shared bounded
// file reads now reject non-regular files before consuming them. Unix-only
// (relies on /dev/zero).
//
// Dedicated test binary: it mutates a process-global limit env var.
#![cfg(unix)]

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;

#[tokio::test]
async fn read_file_rejects_unbounded_special_files() {
    // A tiny cap preserves the original IF-049 setup. The current shared reader
    // rejects the device even earlier, before its misleading zero-byte metadata
    // could participate in the byte-limit check.
    unsafe {
        std::env::set_var("IRONFLOW_MAX_FILE_BYTES", "4096");
    }

    let reg = NodeRegistry::with_builtins();
    let node = reg.get("read_file").unwrap();
    let config = serde_json::json!({ "path": "/dev/zero", "encoding": "base64" });

    let result = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        node.execute(&config, &Context::new()),
    )
    .await
    .expect("read_file must not hang on /dev/zero");

    assert!(result.is_err(), "reading /dev/zero must be rejected");
    let error = result.unwrap_err().to_string();
    assert!(
        error.contains("not a regular file"),
        "error should identify the unsupported file type: {error}"
    );

    unsafe {
        std::env::remove_var("IRONFLOW_MAX_FILE_BYTES");
    }
}
