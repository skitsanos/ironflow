//! Integration tests for command-line surface, configuration precedence and diagnostics.
//!
//! One binary per area keeps the number of linked test executables bounded
//! (IF-162). Every module is an unchanged test file under `tests/`; run one
//! with `cargo test --test cli <module>::`.

#[path = "../test_artifact_restore_cli.rs"]
mod test_artifact_restore_cli;
#[path = "../test_cache_identity_cli.rs"]
mod test_cache_identity_cli;
#[path = "../test_child_result_cli.rs"]
mod test_child_result_cli;
#[path = "../test_cli_list.rs"]
mod test_cli_list;
#[path = "../test_cli_precedence.rs"]
mod test_cli_precedence;
#[path = "../test_cli_run.rs"]
mod test_cli_run;
#[path = "../test_config.rs"]
mod test_config;
#[path = "../test_parallel_literal_cli.rs"]
mod test_parallel_literal_cli;
#[path = "../test_parallel_result_cli.rs"]
mod test_parallel_result_cli;
#[path = "../test_pptx_presentation_cli.rs"]
mod test_pptx_presentation_cli;
#[path = "../test_reused_handlers_cli.rs"]
mod test_reused_handlers_cli;
#[path = "../test_s3_signing_cli.rs"]
mod test_s3_signing_cli;
#[path = "../test_sql_backend_cli.rs"]
mod test_sql_backend_cli;
#[path = "../test_validation_diagnostics.rs"]
mod test_validation_diagnostics;
