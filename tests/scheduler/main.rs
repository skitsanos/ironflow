//! Integration tests for schedule configuration, claims, timing and the scheduler executor.
//!
//! One binary per area keeps the number of linked test executables bounded
//! (IF-162). Every module is an unchanged test file under `tests/`; run one
//! with `cargo test --test scheduler <module>::`.

// Several of these files include the same shared Redis or PostgreSQL support
// module, each as its own private copy, exactly as they did as separate crates.
#![allow(clippy::duplicate_mod)]

#[path = "../test_schedule_claims.rs"]
mod test_schedule_claims;
#[path = "../test_schedule_config.rs"]
mod test_schedule_config;
#[path = "../test_schedule_field_shapes.rs"]
mod test_schedule_field_shapes;
#[path = "../test_schedule_run_records.rs"]
mod test_schedule_run_records;
#[path = "../test_schedule_timing.rs"]
mod test_schedule_timing;
#[path = "../test_schedule_weekdays.rs"]
mod test_schedule_weekdays;
#[path = "../test_schedule_zone_parity.rs"]
mod test_schedule_zone_parity;
#[path = "../test_scheduler_availability.rs"]
mod test_scheduler_availability;
#[path = "../test_scheduler_decisions.rs"]
mod test_scheduler_decisions;
#[path = "../test_scheduler_e2e.rs"]
mod test_scheduler_e2e;
#[path = "../test_scheduler_load_admission.rs"]
mod test_scheduler_load_admission;
#[path = "../test_scheduler_occurrence_fence.rs"]
mod test_scheduler_occurrence_fence;
#[path = "../test_scheduler_skip_records.rs"]
mod test_scheduler_skip_records;
