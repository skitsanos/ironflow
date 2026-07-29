use ironflow::scheduler::config::parse_cron;

#[test]
fn accepts_the_five_field_form_users_write() {
    assert!(parse_cron("0 2 * * *").is_ok());
    assert!(parse_cron("*/15 * * * *").is_ok());
    assert!(parse_cron("30 4 1 * *").is_ok());
}

#[test]
fn rejects_the_six_field_form_rather_than_reinterpreting_it() {
    // `cron` itself accepts this and reads the leading field as seconds, which
    // is never what an author writing standard cron meant.
    let error = parse_cron("0 2 * * * *").unwrap_err();
    assert!(error.contains("five"), "unhelpful message: {error}");
    assert!(
        error.contains("6"),
        "message should name what was supplied: {error}"
    );
}

#[test]
fn rejects_wrong_field_counts_with_an_actionable_message() {
    for expression in ["", "   ", "* * *", "0 0 2 * * * 2026"] {
        let error = parse_cron(expression).unwrap_err();
        assert!(
            error.contains("five"),
            "unhelpful message for {expression:?}: {error}"
        );
    }
}

#[test]
fn rejects_an_unparseable_expression_without_leaking_parser_noise() {
    let error = parse_cron("99 2 * * *").unwrap_err();
    assert!(
        error.contains("99 2 * * *"),
        "message should quote the input: {error}"
    );
    // The parser's own message is a multi-line caret diagram; it must not be
    // pasted into a startup error verbatim.
    assert!(!error.contains('\n'), "message should be one line: {error}");
}

#[test]
fn a_five_field_expression_fires_at_the_top_of_the_minute() {
    use chrono::{TimeZone as _, Utc};
    let schedule = parse_cron("0 2 * * *").unwrap();
    let after = Utc.with_ymd_and_hms(2026, 5, 1, 0, 0, 0).unwrap();
    let next = schedule.after(&after).next().unwrap();
    assert_eq!(next, Utc.with_ymd_and_hms(2026, 5, 1, 2, 0, 0).unwrap());
}
