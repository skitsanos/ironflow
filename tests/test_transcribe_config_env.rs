// This assertion clears the process-global `OPENAI_API_KEY` environment
// variable. `cargo test --lib` runs many test modules concurrently in a
// single process, where that mutation would race with any other thread's
// `std::env::var` call -- concurrent getenv/setenv is a data race and
// undefined behaviour under the Rust 2024 `unsafe` contract, regardless of
// which specific variable each side happens to touch.
// `tests/test_limits_defaults.rs` established this "give the mutation its
// own process" pattern first; this file follows it for the same reason.
//
// `transcribe::config::resolve` and `TranscribeConfig` are `pub(super)` --
// visible only inside `nodes::ai::transcribe` -- so they cannot be called
// from an integration test. This instead drives the node through its public
// `Node::execute` interface via `NodeRegistry::with_builtins()`, which
// exercises the exact same credential-resolution path from the outside.

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;

#[tokio::test(flavor = "current_thread")]
async fn missing_credential_names_the_parameter_and_the_environment_variable() {
    let saved = std::env::var("OPENAI_API_KEY").ok();
    unsafe { std::env::remove_var("OPENAI_API_KEY") };

    let registry = NodeRegistry::with_builtins();
    let node = registry
        .get("transcribe")
        .expect("transcribe node is registered");

    // `resolve()` checks `api_key` before ever touching the filesystem, so
    // this path does not need to exist -- the node will fail on the missing
    // credential before it tries to read the file.
    let config = serde_json::json!({ "path": "/tmp/does-not-need-to-exist.mp3" });
    let result = node.execute(&config, &Context::new()).await;

    if let Some(value) = saved {
        unsafe { std::env::set_var("OPENAI_API_KEY", value) };
    }

    let error = result.unwrap_err().to_string();
    assert!(error.contains("api_key"), "{error}");
    assert!(error.contains("OPENAI_API_KEY"), "{error}");
}
