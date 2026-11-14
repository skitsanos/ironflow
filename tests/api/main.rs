//! Integration tests for HTTP API, SSE, idempotency and webhook routing.
//!
//! One binary per area keeps the number of linked test executables bounded
//! (IF-162). Every module is an unchanged test file under `tests/`; run one
//! with `cargo test --test api <module>::`.

// One copy of the webhook support module for every suite in this binary: its
// process-wide admission lock only serializes the tests that share it.
#[path = "../support/webhook.rs"]
mod webhook_support;

#[path = "../test_api.rs"]
mod test_api;
#[path = "../test_api_errors.rs"]
mod test_api_errors;
#[path = "../test_api_idempotency.rs"]
mod test_api_idempotency;
#[path = "../test_api_sse.rs"]
mod test_api_sse;
#[path = "../test_blank_settings.rs"]
mod test_blank_settings;
#[path = "../test_request_context_depth.rs"]
mod test_request_context_depth;
#[path = "../test_run_id_api.rs"]
mod test_run_id_api;
#[path = "../test_serve_drain.rs"]
mod test_serve_drain;
#[path = "../test_webhook.rs"]
mod test_webhook;
#[path = "../test_webhook_composition.rs"]
mod test_webhook_composition;
#[path = "../test_webhook_header_redaction.rs"]
mod test_webhook_header_redaction;
#[path = "../test_webhook_persistence_boundaries.rs"]
mod test_webhook_persistence_boundaries;
