use chrono::NaiveDateTime;
use ironflow::engine::types::Context;
use ironflow::scheduler::config::ScheduleConfig;
use ironflow::scheduler::timing::{MAX_INSTANTS_PER_TICK, claim_key, due_instants, resolve_local};

fn local(text: &str) -> NaiveDateTime {
    NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M:%S").unwrap()
}

fn daily_at_two(zone: &str) -> ScheduleConfig {
    ScheduleConfig::new("f.lua", "0 2 * * *", Some(zone), None, Context::new()).unwrap()
}

#[test]
fn ordinary_days_resolve_to_the_configured_wall_clock_time() {
    let schedule = daily_at_two("Europe/Berlin");
    let (due, truncated) = due_instants(
        &schedule,
        local("2026-05-01 00:00:00"),
        local("2026-05-02 12:00:00"),
    );

    assert!(!truncated);
    assert_eq!(due.len(), 2);
    assert_eq!(due[0].local, local("2026-05-01 02:00:00"));
    assert_eq!(due[0].instant.to_rfc3339(), "2026-05-01T02:00:00+02:00");
}

#[test]
fn spring_forward_fires_after_the_gap_instead_of_vanishing() {
    // Berlin skips 02:00–03:00 on 2026-03-29; a daily 02:00 job must still run.
    let schedule = daily_at_two("Europe/Berlin");
    let (due, _) = due_instants(
        &schedule,
        local("2026-03-29 00:00:00"),
        local("2026-03-29 23:00:00"),
    );

    assert_eq!(due.len(), 1, "the gap day must still produce a fire");
    assert_eq!(due[0].local, local("2026-03-29 02:00:00"));
    assert_eq!(due[0].instant.to_rfc3339(), "2026-03-29T03:00:00+02:00");

    // Second zone, so the logic is not tuned to Berlin: New York skips
    // 02:00–03:00 on 2026-03-08.
    let schedule = daily_at_two("America/New_York");
    let (due, _) = due_instants(
        &schedule,
        local("2026-03-08 00:00:00"),
        local("2026-03-08 23:00:00"),
    );
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].instant.to_rfc3339(), "2026-03-08T03:00:00-04:00");
}

#[test]
fn fall_back_fires_once_on_the_earlier_instant() {
    // Berlin repeats 02:00–03:00 on 2026-10-25; 02:00 occurs at both
    // 00:00Z (CEST) and 01:00Z (CET).
    let schedule = daily_at_two("Europe/Berlin");
    let (due, _) = due_instants(
        &schedule,
        local("2026-10-25 00:00:00"),
        local("2026-10-25 23:00:00"),
    );

    assert_eq!(due.len(), 1, "the repeated hour must not fire twice");
    assert_eq!(due[0].instant.to_rfc3339(), "2026-10-25T02:00:00+02:00");
    assert_eq!(due[0].instant.naive_utc(), local("2026-10-25 00:00:00"));

    // New York repeats 01:00–02:00 on 2026-11-01, so probe 01:00 there.
    let schedule = ScheduleConfig::new(
        "f.lua",
        "0 1 * * *",
        Some("America/New_York"),
        None,
        Context::new(),
    )
    .unwrap();
    let (due, _) = due_instants(
        &schedule,
        local("2026-11-01 00:00:00"),
        local("2026-11-01 23:00:00"),
    );
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].instant.to_rfc3339(), "2026-11-01T01:00:00-04:00");
}

#[test]
fn the_claim_key_is_local_wall_clock_so_a_repeated_hour_collapses_to_one_claim() {
    let tz = chrono_tz::Europe::Berlin;
    let key = claim_key(tz, local("2026-10-25 02:00:00"));
    assert_eq!(key, "Europe/Berlin@2026-10-25T02:00");

    // The two real instants of the repeated hour must share one key.
    let earlier = resolve_local(tz, local("2026-10-25 02:00:00")).unwrap();
    assert_eq!(claim_key(tz, earlier.naive_local()), key);
}

#[test]
fn distinct_days_and_zones_produce_distinct_keys() {
    let berlin = chrono_tz::Europe::Berlin;
    assert_ne!(
        claim_key(berlin, local("2026-10-25 02:00:00")),
        claim_key(berlin, local("2026-10-26 02:00:00"))
    );
    assert_ne!(
        claim_key(berlin, local("2026-10-25 02:00:00")),
        claim_key(chrono_tz::UTC, local("2026-10-25 02:00:00"))
    );
}

#[test]
fn the_window_is_half_open_so_an_instant_cannot_be_emitted_twice() {
    let schedule = daily_at_two("UTC");
    let (first, _) = due_instants(
        &schedule,
        local("2026-05-01 00:00:00"),
        local("2026-05-01 02:00:00"),
    );
    assert_eq!(first.len(), 1);

    // The next tick starts where the previous one ended.
    let (second, _) = due_instants(
        &schedule,
        local("2026-05-01 02:00:00"),
        local("2026-05-01 12:00:00"),
    );
    assert!(second.is_empty(), "re-emitted an already-evaluated instant");
}

#[test]
fn a_long_window_is_capped_and_reports_the_truncation() {
    let schedule =
        ScheduleConfig::new("f.lua", "* * * * *", Some("UTC"), None, Context::new()).unwrap();
    let (due, truncated) = due_instants(
        &schedule,
        local("2026-05-01 00:00:00"),
        local("2026-05-02 00:00:00"),
    );

    assert_eq!(due.len(), MAX_INSTANTS_PER_TICK);
    assert!(truncated, "silent truncation reads as full coverage");
}

#[test]
fn an_empty_or_inverted_window_yields_nothing() {
    let schedule = daily_at_two("UTC");
    let (due, truncated) = due_instants(
        &schedule,
        local("2026-05-01 12:00:00"),
        local("2026-05-01 06:00:00"),
    );
    assert!(due.is_empty());
    assert!(!truncated);
}
