// IF-049: read_file's size guard trusted metadata().len(), which is 0 for
// special files like /dev/zero, so the read streamed unbounded. The read is now
// bounded by IRONFLOW_MAX_FILE_BYTES. Unix-only (relies on /dev/zero).
//
// Dedicated test binary: it mutates a process-global limit env var.
#![cfg(unix)]

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;

#[tokio::test]
async fn read_file_bounds_unbounded_special_files() {
    // A tiny cap so the bounded read fails almost immediately instead of
    // streaming gigabytes from /dev/zero (which reports length 0).
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

    assert!(
        result.is_err(),
        "reading /dev/zero must be bounded to an error"
    );
    assert!(
        result.unwrap_err().to_string().contains("limit"),
        "error should reference the byte limit"
    );

    unsafe {
        std::env::remove_var("IRONFLOW_MAX_FILE_BYTES");
    }
}
