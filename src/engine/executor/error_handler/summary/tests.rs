use super::*;
use crate::engine::executor::error_handler::StepFailure;
use crate::engine::types::Context;

fn insert(state: &mut ExecutionState, name: &str, message: &str) {
    state.failures.insert(
        name.to_string(),
        StepFailure {
            message: message.to_string(),
            node_type: "test".to_string(),
            output: None,
        },
    );
}

#[test]
fn summary_is_deterministic_and_omits_recovered_failures() {
    let overlay = ExecutionOverlay::default();
    let mut state = ExecutionState::default();
    insert(&mut state, "zeta", "HTTP 400 Bad Request");
    insert(&mut state, "recovered", "must not escape");
    insert(&mut state, "alpha", "boom");
    state.resolve_failure("recovered");
    assert_eq!(
        state.failure_summary(&overlay).as_deref(),
        Some("task 'alpha': boom; task 'zeta': HTTP 400 Bad Request")
    );
    state.resolve_failure("alpha");
    state.resolve_failure("zeta");
    assert!(state.failure_summary(&overlay).is_none());
}

#[test]
fn failed_recovery_keeps_both_reasons_without_dependency_skip_noise() {
    let mut state = ExecutionState::default();
    insert(&mut state, "source", "original boom");
    assert!(state.take_for_recovery("source").is_some());
    insert(&mut state, "recover", "handler boom");
    state.mark_unavailable("downstream");
    assert_eq!(
        state
            .failure_summary(&ExecutionOverlay::default())
            .as_deref(),
        Some("task 'recover': handler boom; task 'source': original boom")
    );
}

#[test]
fn summary_bounds_selection_and_utf8_without_losing_later_reasons() {
    let mut forward = ExecutionState::default();
    let mut reverse = ExecutionState::default();
    for index in 0..12 {
        let name = format!("{index:02}-{}", "\u{e9}".repeat(200));
        insert(&mut forward, &name, &"\u{e9}".repeat(1000));
    }
    for index in (0..12).rev() {
        let name = format!("{index:02}-{}", "\u{e9}".repeat(200));
        insert(&mut reverse, &name, &"\u{e9}".repeat(1000));
    }
    let summary = forward
        .failure_summary(&ExecutionOverlay::default())
        .unwrap();
    assert_eq!(
        Some(summary.clone()),
        reverse.failure_summary(&ExecutionOverlay::default())
    );
    assert!(summary.len() < 8192);
    assert_eq!(summary.matches("task '").count(), MAX_FAILURES);
    assert_eq!(summary.matches(TRUNCATED).count(), MAX_FAILURES * 2);
    assert!(summary.contains("task '07-"));
    assert!(!summary.contains("task '08-"));
    assert!(summary.ends_with("4 additional task failure(s) omitted"));
}

#[test]
fn summary_redacts_names_and_whole_secrets_before_truncation() {
    let secret = "private-overlay-credential".repeat(60);
    let overlay = ExecutionOverlay::new(Context::from([(
        "_credential".to_string(),
        serde_json::json!(secret),
    )]));
    let mut state = ExecutionState::default();
    insert(
        &mut state,
        &format!("step-{secret}"),
        &format!(
            "boom {secret} https://user:password@invalid.test/?api_key=url-secret password=dsn-secret"
        ),
    );
    let summary = state.failure_summary(&overlay).unwrap();
    assert!(summary.contains("task 'step-[REDACTED]': boom [REDACTED]"));
    for forbidden in [
        "private-overlay",
        "user:password",
        "url-secret",
        "dsn-secret",
    ] {
        assert!(!summary.contains(forbidden), "{summary}");
    }
}
