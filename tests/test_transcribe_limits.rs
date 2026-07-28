// The audio cap is enforced before upload, so no server is needed: an oversized
// file must fail without a request being attempted. Dedicated test binary
// because it mutates a process-global limit env var.

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;

#[tokio::test]
async fn transcribe_refuses_audio_above_the_cap() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("big.mp3");
    std::fs::write(&path, vec![0u8; 8192]).unwrap();

    let node = NodeRegistry::with_builtins().get("transcribe").unwrap();
    let config = serde_json::json!({
        "path": path.to_str().unwrap(),
        "provider": "openai_compatible",
        "base_url": "http://127.0.0.1:1",
        "api_key": "k"
    });

    unsafe { std::env::set_var("IRONFLOW_MAX_AUDIO_BYTES", "1024") };
    let error = node
        .execute(&config, &Context::new())
        .await
        .expect_err("oversized audio must be refused")
        .to_string();
    assert!(error.contains("limit"), "{error}");
    assert!(error.contains("transcribe"), "{error}");

    // Under a generous cap the same file reaches the (unreachable) endpoint,
    // proving the cap was the only thing rejecting it.
    unsafe { std::env::set_var("IRONFLOW_MAX_AUDIO_BYTES", "25000000") };
    let error = node
        .execute(&config, &Context::new())
        .await
        .expect_err("connection to port 1 must fail")
        .to_string();
    assert!(
        !error.contains("limit"),
        "cap fired under a generous limit: {error}"
    );

    unsafe { std::env::remove_var("IRONFLOW_MAX_AUDIO_BYTES") };
}
