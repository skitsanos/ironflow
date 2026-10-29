//! Integration tests for Redis-backed storage (requires the `redis` feature).
//!
//! One binary per area keeps the number of linked test executables bounded
//! (IF-162). Every module is an unchanged test file under `tests/`; run one
//! with `cargo test --test storage_redis <module>::`.

// Several of these files include the same shared support or fixture module,
// each as its own private copy, exactly as they did as separate crates.
#![allow(clippy::duplicate_mod)]

#[path = "../test_redis_atomicity.rs"]
mod test_redis_atomicity;
#[path = "../test_redis_lease_atomicity.rs"]
mod test_redis_lease_atomicity;
#[path = "../test_redis_store.rs"]
mod test_redis_store;
