//! Integration tests for PostgreSQL-backed storage (requires the `postgres` feature).
//!
//! One binary per area keeps the number of linked test executables bounded
//! (IF-162). Every module is an unchanged test file under `tests/`; run one
//! with `cargo test --test storage_postgres <module>::`.

#[path = "../test_db_postgres_types.rs"]
mod test_db_postgres_types;
#[path = "../test_sql_schema_concurrency.rs"]
mod test_sql_schema_concurrency;
