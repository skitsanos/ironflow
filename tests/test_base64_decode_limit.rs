// IF-050: base64_decode writing to output_file must honor IRONFLOW_MAX_FILE_BYTES
// so it cannot be used as an unbounded arbitrary-path write sink.
//
// Dedicated test binary: it mutates a process-global limit env var.

use base64::Engine;
use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;

#[tokio::test]
async fn base64_decode_file_write_is_size_capped() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("decoded.bin");
    let payload = base64::engine::general_purpose::STANDARD.encode(vec![0u8; 8192]);

    let reg = NodeRegistry::with_builtins();
    let node = reg.get("base64_decode").unwrap();
    let config = serde_json::json!({
        "input": payload,
        "output_file": out.to_str().unwrap(),
    });

    unsafe {
        std::env::set_var("IRONFLOW_MAX_FILE_BYTES", "1024");
    }
    let result = node.execute(&config, &Context::new()).await;
    assert!(
        result.is_err(),
        "an 8 KB decode over a 1 KB cap must be rejected"
    );
    assert!(
        result.unwrap_err().to_string().contains("limit"),
        "error should reference the byte limit"
    );
    assert!(!out.exists(), "the oversized file must not be written");

    unsafe {
        std::env::remove_var("IRONFLOW_MAX_FILE_BYTES");
    }
}
