//! Integration tests for shipped Lua examples, their catalog, fixtures and schema examples.
//!
//! One binary per area keeps the number of linked test executables bounded
//! (IF-162). Every module is an unchanged test file under `tests/`; run one
//! with `cargo test --test examples <module>::`.

#[path = "../test_example_catalog.rs"]
mod test_example_catalog;
#[path = "../test_example_contracts.rs"]
mod test_example_contracts;
#[path = "../test_example_fixtures.rs"]
mod test_example_fixtures;
#[path = "../test_examples.rs"]
mod test_examples;
#[path = "../test_schema_examples.rs"]
mod test_schema_examples;
