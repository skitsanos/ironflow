//! Bounded work of the diagnostic scrubber (IF-177): a linear scan and a cap
//! on the scanned prefix. Before the fix a message of repeated sensitive
//! words cost quadratic time: 240 KB took 334 s.

use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use super::*;

const MEBIBYTE: usize = 1024 * 1024;

/// Generous for an unoptimized build under load; the quadratic scan needed
/// hours for the same input.
const BUDGET: Duration = Duration::from_secs(10);

/// Run `work` on its own thread and fail instead of hanging when it overruns.
#[track_caller]
fn within_budget<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> T {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let _ = sender.send(work());
    });
    receiver
        .recv_timeout(BUDGET)
        .unwrap_or_else(|_| panic!("scrubbing did not finish within {BUDGET:?}"))
}

fn repeated(unit: &str) -> String {
    unit.repeat(MEBIBYTE / unit.len() + 1)
}

#[test]
fn one_mebibyte_of_sensitive_words_is_scrubbed_within_budget() {
    let text = repeated("token ");

    let redacted = within_budget(move || redact_sensitive_text(&text));

    assert!(
        redacted.starts_with("token token "),
        "{:?}",
        &redacted[..32]
    );
}

#[test]
fn one_mebibyte_of_assignments_and_urls_is_scrubbed_within_budget() {
    for unit in [
        "password=hunter2 ",
        "\"api_key\": \"hunter2\", ",
        "see https://user:hunter2@example.test/path?token=hunter2 ",
        "postgres://user:hunter2@db.test/app ",
    ] {
        let text = repeated(unit);

        let redacted = within_budget(move || redact_sensitive_text(&text));

        assert!(!redacted.contains("hunter2"), "unit: {unit}");
    }
}

/// The cap alone must not be what keeps the public function fast: the scan
/// stays linear for a mebibyte of each shape that used to restart a search or
/// that could make this implementation look backwards.
#[test]
fn the_scan_is_linear_without_the_cap() {
    for unit in [
        "token ",
        "token_",
        "token=x ",
        "token:,",
        "\\\"token\\\":\\\"x\\\",",
        "token=\\\\ ",
        "https://a.test/b ",
        "redis://u:p@h/0 ",
    ] {
        let text = repeated(unit);

        let redacted = within_budget(move || scrub(&text, false));

        assert!(!redacted.is_empty(), "unit: {unit}");
    }

    let text = format!("{}\"token\": \"hunter2\"", "\\".repeat(MEBIBYTE));
    let redacted = within_budget(move || scrub(&text, false));
    assert!(redacted.ends_with("\"token\": \"[REDACTED]\""));
}

#[test]
fn a_diagnostic_within_the_cap_is_kept_whole() {
    let text = "x".repeat(MAX_DIAGNOSTIC_BYTES);

    assert_eq!(redact_sensitive_text(&text), text);
}

#[test]
fn text_beyond_the_cap_is_replaced_by_a_marker() {
    let kept = "x".repeat(MAX_DIAGNOSTIC_BYTES);

    let redacted = redact_sensitive_text(&format!("{kept} password=hunter2"));

    assert_eq!(redacted, format!("{kept}{TRUNCATED}"));
}

#[test]
fn the_cap_never_splits_a_character() {
    // Every boundary after the first byte is odd; the cap is even.
    let text = format!("x{}", "\u{e9}".repeat(MAX_DIAGNOSTIC_BYTES));

    let redacted = redact_sensitive_text(&text);

    assert_eq!(redacted.len(), MAX_DIAGNOSTIC_BYTES - 1 + TRUNCATED.len());
    assert!(redacted.ends_with(TRUNCATED));
}

#[test]
fn a_url_cut_by_the_cap_is_replaced_whole() {
    // Without its `@` the remainder parses as a host and a port, and would
    // be kept: `postgres://alice:123`.
    let kept_url_bytes = "postgres://alice:123".len();
    let padding = "x".repeat(MAX_DIAGNOSTIC_BYTES - kept_url_bytes - 1);
    let text = format!("{padding} postgres://alice:123456@db.internal/app tail");

    let redacted = redact_sensitive_text(&text);

    assert_eq!(redacted, format!("{padding} {REDACTED_URL}{TRUNCATED}"));
}

#[test]
fn a_complete_url_before_the_cut_keeps_its_host() {
    let text = format!(
        "connect postgres://alice:pw@db.internal/app failed {}",
        "x".repeat(MAX_DIAGNOSTIC_BYTES)
    );

    let redacted = redact_sensitive_text(&text);

    assert!(redacted.starts_with("connect postgres://db.internal/app failed xxx"));
    assert!(redacted.ends_with(TRUNCATED));
}

#[test]
fn an_assignment_cut_by_the_cap_loses_its_partial_value() {
    let kept_assignment_bytes = "password=hunter".len();
    let padding = "x".repeat(MAX_DIAGNOSTIC_BYTES - kept_assignment_bytes - 1);
    let text = format!("{padding} password=hunter2-long-secret tail");

    let redacted = redact_sensitive_text(&text);

    assert_eq!(
        redacted,
        format!("{padding} password={REDACTED}{TRUNCATED}")
    );
}

#[test]
fn scrubbing_truncated_output_again_stays_bounded_and_clean() {
    let once = redact_sensitive_text(&repeated("password=hunter2 "));
    assert!(once.len() > MAX_DIAGNOSTIC_BYTES, "redaction grew the text");

    let twice = redact_sensitive_text(&once);

    assert!(!twice.contains("hunter2"));
    assert!(twice.len() <= MAX_DIAGNOSTIC_BYTES + REDACTED.len() + TRUNCATED.len());
    assert!(twice.ends_with(TRUNCATED));
}
