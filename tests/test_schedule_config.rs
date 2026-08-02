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

#[test]
fn restricted_day_of_month_and_weekday_use_traditional_or_semantics() {
    use chrono::{TimeZone as _, Utc};

    let schedule = parse_cron("0 4 1 * Fri").unwrap();
    let after = Utc.with_ymd_and_hms(2026, 5, 29, 4, 0, 0).unwrap();
    let next = schedule.after(&after).take(2).collect::<Vec<_>>();

    assert_eq!(
        next,
        [
            Utc.with_ymd_and_hms(2026, 6, 1, 4, 0, 0).unwrap(),
            Utc.with_ymd_and_hms(2026, 6, 5, 4, 0, 0).unwrap(),
        ]
    );
}

#[test]
fn a_date_matching_both_day_fields_is_emitted_once() {
    use chrono::{TimeZone as _, Utc};

    let schedule = parse_cron("0 4 1 * Fri").unwrap();
    let after = Utc.with_ymd_and_hms(2026, 4, 30, 4, 0, 0).unwrap();
    let next = schedule.after(&after).take(2).collect::<Vec<_>>();

    assert_eq!(next[0], Utc.with_ymd_and_hms(2026, 5, 1, 4, 0, 0).unwrap());
    assert_eq!(next[1], Utc.with_ymd_and_hms(2026, 5, 8, 4, 0, 0).unwrap());
}

use ironflow::engine::types::Context;
use ironflow::scheduler::config::ScheduleConfig;

fn config(cron: &str, timezone: Option<&str>) -> Result<ScheduleConfig, String> {
    ScheduleConfig::new("nightly.lua", cron, timezone, None, Context::new())
}

#[test]
fn defaults_are_utc_and_five_minutes_of_grace() {
    let schedule = config("0 2 * * *", None).unwrap();
    assert_eq!(schedule.flow(), "nightly.lua");
    assert_eq!(schedule.timezone(), chrono_tz::UTC);
    assert_eq!(schedule.grace_seconds(), 300);
    assert_eq!(schedule.cron_source(), "0 2 * * *");
    assert!(schedule.context().is_empty());
}

#[test]
fn named_timezones_are_accepted_and_unknown_ones_fail() {
    assert_eq!(
        config("0 2 * * *", Some("Europe/Berlin"))
            .unwrap()
            .timezone(),
        chrono_tz::Europe::Berlin
    );

    let error = config("0 2 * * *", Some("Europe/Atlantis")).unwrap_err();
    assert!(error.contains("Europe/Atlantis"), "{error}");
    assert!(error.contains("IANA"), "{error}");
}

#[test]
fn an_empty_flow_path_is_rejected() {
    let error = ScheduleConfig::new("   ", "0 2 * * *", None, None, Context::new()).unwrap_err();
    assert!(error.contains("flow"), "{error}");
}

#[test]
fn grace_below_one_tick_is_rejected_rather_than_silently_flaky() {
    let error =
        ScheduleConfig::new("f.lua", "0 2 * * *", None, Some(30), Context::new()).unwrap_err();
    assert!(error.contains("60"), "{error}");
    assert!(
        error.contains("30 seconds"),
        "message should explain why: {error}"
    );

    assert!(ScheduleConfig::new("f.lua", "0 2 * * *", None, Some(60), Context::new()).is_ok());
}

#[test]
fn schedule_strings_context_and_grace_are_bounded() {
    use ironflow::scheduler::config::{
        MAX_CRON_EXPRESSION_BYTES, MAX_GRACE_SECONDS, MAX_SCHEDULE_CONTEXT_BYTES,
        MAX_SCHEDULE_FLOW_BYTES, MAX_SCHEDULE_NAME_BYTES, MAX_SCHEDULE_TIMEZONE_BYTES,
        validate_schedule_name,
    };

    let error = ScheduleConfig::new(
        "x".repeat(MAX_SCHEDULE_FLOW_BYTES + 1),
        "0 2 * * *",
        None,
        None,
        Context::new(),
    )
    .unwrap_err();
    assert!(
        error.contains("flow path") && error.contains("limit"),
        "{error}"
    );

    let error = ScheduleConfig::new(
        "f.lua",
        &"x".repeat(MAX_CRON_EXPRESSION_BYTES + 1),
        None,
        None,
        Context::new(),
    )
    .unwrap_err();
    assert!(error.contains("cron") && error.contains("limit"), "{error}");

    let timezone = "x".repeat(MAX_SCHEDULE_TIMEZONE_BYTES + 1);
    let error = ScheduleConfig::new("f.lua", "0 2 * * *", Some(&timezone), None, Context::new())
        .unwrap_err();
    assert!(
        error.contains("timezone") && error.contains("limit"),
        "{error}"
    );

    let error = ScheduleConfig::new(
        "f.lua",
        "0 2 * * *",
        None,
        Some(MAX_GRACE_SECONDS + 1),
        Context::new(),
    )
    .unwrap_err();
    assert!(
        error.contains("grace_seconds") && error.contains("exceed"),
        "{error}"
    );

    let mut context = Context::new();
    context.insert(
        "payload".to_string(),
        serde_json::json!("x".repeat(MAX_SCHEDULE_CONTEXT_BYTES)),
    );
    let error = ScheduleConfig::new("f.lua", "0 2 * * *", None, None, context).unwrap_err();
    assert!(
        error.contains("context") && error.contains("limit"),
        "{error}"
    );

    let long_name = "n".repeat(MAX_SCHEDULE_NAME_BYTES + 1);
    assert!(
        validate_schedule_name(&long_name)
            .unwrap_err()
            .contains("limit")
    );
    assert!(
        validate_schedule_name("bad\nname")
            .unwrap_err()
            .contains("control")
    );
}

#[test]
fn reserved_context_keys_are_refused_at_startup() {
    for reserved in ["_schedule", "_flow_dir"] {
        let mut ctx = Context::new();
        ctx.insert(reserved.to_string(), serde_json::json!("hijacked"));
        let error = ScheduleConfig::new("f.lua", "0 2 * * *", None, None, ctx).unwrap_err();
        assert!(error.contains(reserved), "{error}");
    }
}

#[test]
fn claim_ttl_always_outlives_the_grace_window() {
    const WEEK: u64 = 7 * 24 * 3600;

    let short = ScheduleConfig::new("f.lua", "0 2 * * *", None, Some(300), Context::new()).unwrap();
    assert_eq!(short.claim_ttl_seconds(), WEEK);

    let long = ScheduleConfig::new(
        "f.lua",
        "0 2 * * *",
        None,
        Some(ironflow::scheduler::config::MAX_GRACE_SECONDS),
        Context::new(),
    )
    .unwrap();
    assert!(long.claim_ttl_seconds() > long.grace_seconds());
}

#[test]
fn deserializes_the_documented_yaml_shape() {
    let yaml = r#"
flow: reports/nightly.lua
cron: "0 2 * * *"
timezone: "Europe/Berlin"
grace_seconds: 3600
context:
  region: "eu"
"#;
    let schedule: ScheduleConfig = noyalib::compat::serde_yaml::from_str(yaml).unwrap();
    assert_eq!(schedule.flow(), "reports/nightly.lua");
    assert_eq!(schedule.timezone(), chrono_tz::Europe::Berlin);
    assert_eq!(schedule.grace_seconds(), 3600);
    assert_eq!(schedule.context()["region"], serde_json::json!("eu"));
}

#[test]
fn deserialization_rejects_unknown_and_invalid_fields() {
    let unknown = noyalib::compat::serde_yaml::from_str::<ScheduleConfig>(
        "flow: f.lua\ncron: \"0 2 * * *\"\ntypo_field: 1\n",
    );
    assert!(unknown.is_err());

    let bad_zone = noyalib::compat::serde_yaml::from_str::<ScheduleConfig>(
        "flow: f.lua\ncron: \"0 2 * * *\"\ntimezone: \"Nowhere/Nothing\"\n",
    )
    .unwrap_err()
    .to_string();
    assert!(bad_zone.contains("Nowhere/Nothing"), "{bad_zone}");
}
