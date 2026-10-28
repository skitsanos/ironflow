//! Integration tests that mutate process-global environment variables.
//!
//! Each module below sets or clears `IRONFLOW_*` or provider variables while
//! it runs. Concurrent `setenv`/`getenv` is undefined behaviour, so this
//! binary must run its tests one at a time:
//!
//! ```text
//! cargo test --test serial -- --test-threads=1
//! ```
//!
//! `scripts/test_rust_targets.ts`, the integration gate and CI pass that flag
//! automatically; `single_threaded_harness` below fails fast when it is
//! missing. Tests that also need a pristine process-wide `OnceLock` keep
//! their own binary (see the `[[test]]` entries in `Cargo.toml`).

// Several of these files include the same shared support or fixture module,
// each as its own private copy, exactly as they did as separate crates.
#![allow(clippy::duplicate_mod)]

#[test]
fn single_threaded_harness() {
    let args: Vec<String> = std::env::args().collect();
    let flag = args
        .windows(2)
        .any(|pair| pair[0] == "--test-threads" && pair[1] == "1")
        || args.iter().any(|arg| arg == "--test-threads=1");
    let env = std::env::var("RUST_TEST_THREADS")
        .map(|v| v == "1")
        .unwrap_or(false);
    assert!(
        flag || env,
        "the `serial` test binary mutates process environment variables and must run with \
         `cargo test --test serial -- --test-threads=1` (or RUST_TEST_THREADS=1)"
    );
}

#[path = "../test_ai_nodes.rs"]
mod test_ai_nodes;
#[path = "../test_artifact_nodes.rs"]
mod test_artifact_nodes;
#[path = "../test_base64_decode_limit.rs"]
mod test_base64_decode_limit;
#[path = "../test_cache_nodes.rs"]
mod test_cache_nodes;
#[path = "../test_env_allowlist.rs"]
mod test_env_allowlist;
#[path = "../test_extract_limits.rs"]
mod test_extract_limits;
#[path = "../test_extract_nodes.rs"]
mod test_extract_nodes;
#[path = "../test_extract_pdf_limits.rs"]
mod test_extract_pdf_limits;
#[path = "../test_extract_pptx_limits.rs"]
mod test_extract_pptx_limits;
#[path = "../test_extract_pptx_media.rs"]
mod test_extract_pptx_media;
#[path = "../test_extract_pptx_relationships.rs"]
mod test_extract_pptx_relationships;
#[path = "../test_extract_text_limits.rs"]
mod test_extract_text_limits;
#[path = "../test_extract_word_safety.rs"]
mod test_extract_word_safety;
#[path = "../test_flow_source_limits.rs"]
mod test_flow_source_limits;
#[path = "../test_http_artifacts.rs"]
mod test_http_artifacts;
#[path = "../test_http_nodes.rs"]
mod test_http_nodes;
#[path = "../test_image_limits_defaults.rs"]
mod test_image_limits_defaults;
#[path = "../test_llm_artifact_messages.rs"]
mod test_llm_artifact_messages;
#[path = "../test_lua_limits.rs"]
mod test_lua_limits;
#[path = "../test_lua_preemption.rs"]
mod test_lua_preemption;
#[path = "../test_pdf_actual_text_limits.rs"]
mod test_pdf_actual_text_limits;
#[path = "../test_pdf_image_nodes.rs"]
mod test_pdf_image_nodes;
#[path = "../test_pdf_limits_defaults.rs"]
mod test_pdf_limits_defaults;
#[path = "../test_pdf_merge_safety.rs"]
mod test_pdf_merge_safety;
#[path = "../test_read_file_special.rs"]
mod test_read_file_special;
#[path = "../test_run_deadline.rs"]
mod test_run_deadline;
#[path = "../test_send_email_node.rs"]
mod test_send_email_node;
#[path = "../test_shell_markdown_nodes.rs"]
mod test_shell_markdown_nodes;
#[path = "../test_slack_notification_node.rs"]
mod test_slack_notification_node;
#[path = "../test_transcribe_config_env.rs"]
mod test_transcribe_config_env;
#[path = "../test_transcribe_limits.rs"]
mod test_transcribe_limits;
#[path = "../test_webhook_security.rs"]
mod test_webhook_security;
#[path = "../test_write_file_safety.rs"]
mod test_write_file_safety;
