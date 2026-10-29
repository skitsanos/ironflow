//! Integration tests for state and event stores that need no optional backend feature.
//!
//! One binary per area keeps the number of linked test executables bounded
//! (IF-162). Every module is an unchanged test file under `tests/`; run one
//! with `cargo test --test storage <module>::`.

// Several of these files include the same shared support or fixture module,
// each as its own private copy, exactly as they did as separate crates.
#![allow(clippy::duplicate_mod)]

#[path = "../test_artifact_retention.rs"]
mod test_artifact_retention;
#[path = "../test_cache_identity.rs"]
mod test_cache_identity;
#[path = "../test_event_store.rs"]
mod test_event_store;
#[path = "../test_json_store_security.rs"]
mod test_json_store_security;
#[path = "../test_sql_context_concurrency.rs"]
mod test_sql_context_concurrency;
#[path = "../test_sql_event_migration.rs"]
mod test_sql_event_migration;
#[path = "../test_sql_state_concurrency.rs"]
mod test_sql_state_concurrency;
#[path = "../test_state_stores.rs"]
mod test_state_stores;
