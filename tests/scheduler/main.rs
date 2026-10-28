//! Integration tests for schedule configuration, claims, timing and the scheduler executor.
//!
//! One binary per area keeps the number of linked test executables bounded
//! (IF-162). Every module is an unchanged test file under `tests/`; run one
//! with `cargo test --test scheduler <module>::`.

#[path = "../test_schedule_claims.rs"]
mod test_schedule_claims;
#[path = "../test_schedule_config.rs"]
mod test_schedule_config;
#[path = "../test_schedule_timing.rs"]
mod test_schedule_timing;
#[path = "../test_scheduler_availability.rs"]
mod test_scheduler_availability;
#[path = "../test_scheduler_decisions.rs"]
mod test_scheduler_decisions;
#[path = "../test_scheduler_e2e.rs"]
mod test_scheduler_e2e;
