//! Integration tests for HTTP API, SSE, idempotency and webhook routing.
//!
//! One binary per area keeps the number of linked test executables bounded
//! (IF-162). Every module is an unchanged test file under `tests/`; run one
//! with `cargo test --test api <module>::`.

#[path = "../test_api.rs"]
mod test_api;
#[path = "../test_api_errors.rs"]
mod test_api_errors;
#[path = "../test_api_idempotency.rs"]
mod test_api_idempotency;
#[path = "../test_api_sse.rs"]
mod test_api_sse;
#[path = "../test_run_id_api.rs"]
mod test_run_id_api;
#[path = "../test_webhook.rs"]
mod test_webhook;
#[path = "../test_webhook_composition.rs"]
mod test_webhook_composition;
#[path = "../test_webhook_persistence_boundaries.rs"]
mod test_webhook_persistence_boundaries;
