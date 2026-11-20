//! IF-231: the instants a schedule fires on and the claim keys it writes are
//! compared with a recording taken from the `chrono-tz` implementation.
//!
//! `tests/scheduler/fixtures/zone_parity.txt` was recorded before named zones
//! moved to `jiff`. Replicas and existing stores compare claim keys byte for
//! byte, so a difference here is a scheduler regression unless the zone data
//! itself changed; `test_schedule_zones.rs` pins the IANA release.
//!
//! To record again after a reviewed zone data update, run this test with
//! `IRONFLOW_RECORD_ZONE_PARITY=1` and review the fixture diff line by line:
//! only zones and years the new IANA release changed may differ.

use std::fmt::Write as _;
use std::path::PathBuf;

use ironflow::engine::types::Context;
use ironflow::scheduler::config::ScheduleConfig;
use ironflow::scheduler::timing::{DueInstant, claim_key, due_instants, resolve_local};
use jiff::civil::DateTime;
use jiff::{SignedDuration, ZonedDateTime};

/// Windows recorded occurrence by occurrence: `(zone, cron, after, through)`.
const EXACT: &[(&str, &str, &str, &str)] = &[
    // Spring-forward gaps: every matched time inside one coalesces.
    (
        "Europe/Berlin",
        "*/15 1-3 * * *",
        "2026-03-28 00:00",
        "2026-03-30 00:00",
    ),
    (
        "America/New_York",
        "0 2 * * *",
        "2026-03-06 00:00",
        "2026-03-10 00:00",
    ),
    (
        "America/New_York",
        "*/20 2 * * *",
        "2026-03-07 12:00",
        "2026-03-08 12:00",
    ),
    (
        "Australia/Lord_Howe",
        "*/10 1-2 * * *",
        "2026-10-03 00:00",
        "2026-10-05 00:00",
    ),
    (
        "Pacific/Chatham",
        "*/15 2-3 * * *",
        "2026-09-26 00:00",
        "2026-09-28 00:00",
    ),
    (
        "America/Havana",
        "*/20 0 * * *",
        "2026-03-07 00:00",
        "2026-03-09 00:00",
    ),
    (
        "Antarctica/Troll",
        "*/30 0-3 * * *",
        "2026-03-28 12:00",
        "2026-03-29 12:00",
    ),
    (
        "America/Sao_Paulo",
        "*/30 0 * * *",
        "2018-11-03 00:00",
        "2018-11-05 00:00",
    ),
    // Fall-back folds: a repeated wall-clock time is one claim.
    (
        "Europe/Berlin",
        "*/15 1-3 * * *",
        "2026-10-24 00:00",
        "2026-10-26 00:00",
    ),
    (
        "America/New_York",
        "30 1 * * *",
        "2026-10-30 00:00",
        "2026-11-03 00:00",
    ),
    (
        "Australia/Lord_Howe",
        "*/10 1-2 * * *",
        "2026-04-04 00:00",
        "2026-04-06 00:00",
    ),
    (
        "Pacific/Chatham",
        "*/15 2-3 * * *",
        "2026-04-04 00:00",
        "2026-04-06 00:00",
    ),
    (
        "America/Havana",
        "*/20 0 * * *",
        "2026-10-31 00:00",
        "2026-11-02 00:00",
    ),
    (
        "Europe/London",
        "*/30 1 * * *",
        "2026-10-24 00:00",
        "2026-10-26 00:00",
    ),
    // IF-180 weekday numbering, in a zone whose weekday leads UTC.
    (
        "Pacific/Auckland",
        "0 9 * * 0",
        "2026-11-01 00:00",
        "2026-12-01 00:00",
    ),
    (
        "Pacific/Auckland",
        "0 9 * * 7",
        "2026-11-01 00:00",
        "2026-12-01 00:00",
    ),
    (
        "Pacific/Auckland",
        "0 9 * * 1",
        "2026-11-01 00:00",
        "2026-12-01 00:00",
    ),
    (
        "Pacific/Auckland",
        "0 9 * * 6,0",
        "2026-11-01 00:00",
        "2026-12-01 00:00",
    ),
    (
        "America/Los_Angeles",
        "0 22 * * SUN",
        "2026-11-01 00:00",
        "2026-12-01 00:00",
    ),
    (
        "UTC",
        "30 6 * * 1-5",
        "2026-11-01 00:00",
        "2026-11-15 00:00",
    ),
    (
        "UTC",
        "30 6 * * 5-7",
        "2026-11-01 00:00",
        "2026-11-15 00:00",
    ),
    // Day-of-month and day-of-week are a union when both are restricted.
    (
        "Europe/London",
        "0 12 13 * 5",
        "2026-01-01 00:00",
        "2027-01-01 00:00",
    ),
    (
        "Europe/London",
        "0 0 1,15 * 1",
        "2026-01-01 00:00",
        "2026-07-01 00:00",
    ),
    (
        "Asia/Tokyo",
        "15 3 29 2 0",
        "2024-01-01 00:00",
        "2024-04-01 00:00",
    ),
    // Offsets that are not whole hours.
    (
        "Asia/Kolkata",
        "0 */6 * * *",
        "2026-06-01 00:00",
        "2026-06-04 00:00",
    ),
    (
        "Asia/Kathmandu",
        "0 */6 * * *",
        "2026-06-01 00:00",
        "2026-06-04 00:00",
    ),
    (
        "America/St_Johns",
        "*/30 1-2 * * *",
        "2026-03-07 12:00",
        "2026-03-08 12:00",
    ),
    (
        "America/St_Johns",
        "*/30 1-2 * * *",
        "2026-10-31 12:00",
        "2026-11-01 12:00",
    ),
    (
        "Australia/Eucla",
        "0 */8 * * *",
        "2026-06-01 00:00",
        "2026-06-03 00:00",
    ),
    (
        "Asia/Kathmandu",
        "*/15 0 * * *",
        "1985-12-31 12:00",
        "1986-01-01 12:00",
    ),
    // A zone that skipped a whole day: the gap is longer than the probe.
    (
        "Pacific/Apia",
        "0 */4 * * *",
        "2011-12-28 00:00",
        "2012-01-01 00:00",
    ),
    (
        "Pacific/Apia",
        "0 12 30 12 *",
        "2009-01-01 00:00",
        "2014-01-01 00:00",
    ),
    (
        "Pacific/Kiritimati",
        "0 */6 * * *",
        "1994-12-29 00:00",
        "1995-01-03 00:00",
    ),
    (
        "Pacific/Kwajalein",
        "0 */6 * * *",
        "1993-08-19 00:00",
        "1993-08-23 00:00",
    ),
    // The last year `cron` enumerates, and one window after it: empty.
    (
        "Europe/Berlin",
        "0 2 * * *",
        "2099-03-27 00:00",
        "2099-03-31 00:00",
    ),
    (
        "Europe/Berlin",
        "0 2 * * *",
        "2099-10-23 00:00",
        "2099-10-27 00:00",
    ),
    (
        "America/New_York",
        "30 1,2 * * *",
        "2150-03-07 00:00",
        "2150-03-10 00:00",
    ),
    // `cron` enumerates nothing before 1970 either; the first years follow.
    (
        "America/New_York",
        "0 2 * * *",
        "1969-04-25 00:00",
        "1969-04-29 00:00",
    ),
    (
        "Europe/London",
        "0 3 * * *",
        "1971-10-29 00:00",
        "1971-11-02 00:00",
    ),
    (
        "Africa/Monrovia",
        "0 0 * * *",
        "1972-01-05 00:00",
        "1972-01-09 00:00",
    ),
];

/// Wall-clock times resolved one by one, outside the years `cron` enumerates
/// (1970 to 2100) as well as inside: `(zone, first, step minutes, count)`.
const RESOLVE: &[(&str, &str, i64, i64)] = &[
    // Before 1970, including local mean time and offsets with seconds.
    ("America/New_York", "1969-04-27 01:30", 15, 8),
    ("America/New_York", "1969-10-26 00:30", 15, 10),
    ("America/New_York", "1945-08-14 17:00", 30, 6),
    ("America/New_York", "1883-11-18 11:50", 2, 12),
    ("Europe/London", "1968-02-18 01:30", 15, 8),
    ("Europe/London", "1971-10-31 01:30", 15, 10),
    ("Europe/Amsterdam", "1937-06-30 23:30", 10, 8),
    ("Europe/Amsterdam", "1940-05-15 23:30", 20, 10),
    ("Europe/Paris", "1911-03-10 23:40", 5, 8),
    ("Europe/Berlin", "1945-05-24 00:30", 30, 10),
    ("Europe/Dublin", "1916-05-21 01:30", 15, 8),
    ("Asia/Kolkata", "1942-08-31 23:00", 30, 8),
    ("Africa/Monrovia", "1972-01-06 23:30", 15, 8),
    ("Asia/Kathmandu", "1985-12-31 23:30", 5, 10),
    ("Pacific/Apia", "1892-07-04 00:00", 360, 10),
    ("Australia/Sydney", "1895-01-31 23:50", 2, 10),
    // A day that never happened: beyond the gap probe, then within it.
    ("Pacific/Apia", "2011-12-29 12:00", 120, 20),
    ("Pacific/Kiritimati", "1994-12-30 12:00", 120, 20),
    ("Pacific/Kwajalein", "1993-08-20 12:00", 120, 20),
    // The last years before 2100, and zones without rules far beyond them.
    ("Europe/Berlin", "2099-03-29 01:30", 15, 8),
    ("Europe/Berlin", "2099-10-25 01:30", 15, 10),
    ("America/New_York", "2099-03-08 01:30", 15, 8),
    ("America/New_York", "2099-11-01 00:30", 15, 10),
    ("Australia/Lord_Howe", "2099-10-04 01:30", 10, 8),
    ("Australia/Lord_Howe", "2099-04-05 01:00", 10, 10),
    ("Asia/Kolkata", "9000-01-01 00:00", 720, 4),
    ("UTC", "9999-12-31 22:00", 60, 2),
];

/// Instants converted to wall-clock time: `(zone, first UTC, step minutes,
/// count)`. The scheduler does this for every evaluation window.
const LOCAL_TIME: &[(&str, &str, i64, i64)] = &[
    ("Europe/Berlin", "2026-03-29 00:30", 15, 6),
    ("Europe/Berlin", "2026-10-24 23:30", 30, 6),
    ("America/New_York", "2026-11-01 04:30", 30, 6),
    ("Australia/Lord_Howe", "2026-10-03 15:00", 15, 6),
    ("Pacific/Chatham", "2026-04-04 13:30", 15, 6),
    ("Asia/Kathmandu", "2026-06-01 00:00", 60, 3),
    ("Pacific/Apia", "2011-12-30 08:00", 60, 5),
    ("America/New_York", "1883-11-18 16:50", 5, 6),
    ("Europe/Amsterdam", "1937-06-30 23:00", 20, 6),
    ("America/New_York", "1969-10-26 05:00", 30, 5),
    ("Europe/Berlin", "2099-03-29 00:30", 15, 6),
    ("America/New_York", "2099-11-01 05:00", 30, 5),
    ("Asia/Kolkata", "9000-01-01 00:00", 720, 2),
    ("Europe/London", "1970-01-01 00:00", 60, 2),
    ("Etc/GMT+5", "2026-01-01 00:00", 60, 2),
];

/// Zones swept year by year.
const SWEEP_ZONES: &[&str] = &[
    "UTC",
    "Etc/GMT+5",
    "US/Eastern",
    "Europe/Berlin",
    "Europe/London",
    "Europe/Dublin",
    "Europe/Moscow",
    "Europe/Istanbul",
    "America/New_York",
    "America/Los_Angeles",
    "America/Phoenix",
    "America/St_Johns",
    "America/Sao_Paulo",
    "America/Santiago",
    "America/Havana",
    "America/Nuuk",
    "Africa/Cairo",
    "Africa/Johannesburg",
    "Asia/Tehran",
    "Asia/Kolkata",
    "Asia/Kathmandu",
    "Asia/Tokyo",
    "Asia/Amman",
    "Australia/Sydney",
    "Australia/Adelaide",
    "Australia/Lord_Howe",
    "Pacific/Auckland",
    "Pacific/Chatham",
    "Pacific/Apia",
    "Pacific/Kiritimati",
    "Antarctica/Troll",
];

const SWEEP_EXPRESSIONS: &[&str] = &[
    "0 2 * * *",
    "30 1 * * *",
    "*/15 0-3 * * 0",
    "45 23 * * 1-5",
    "0 */6 13 * 5",
    "0 0 1 * *",
];

const SWEEP_YEARS: std::ops::Range<i32> = 2012..2036;

/// The only recorded lines this implementation does not reproduce, as
/// `(recorded, now)`. The recording itself is unchanged.
///
/// `jiff::Timestamp` ends at 9999-12-30T22:00:00.999999999Z, so that every
/// instant has a wall-clock time under every legal offset. The recorded
/// implementation's instants ran on for 250,000 years. A wall-clock time in
/// the last 26 hours of the year 9999 therefore has no instant any more. No
/// schedule can reach one: the cron evaluator stops after 2100. Only a direct
/// caller of `resolve_local` can observe it (IF-231).
const BEYOND_THE_LAST_INSTANT: &[(&str, &str)] = &[
    (
        "  9999-12-31T22:00:00 | 9999-12-31T22:00:00+00:00 | UTC@9999-12-31T22:00",
        "  9999-12-31T22:00:00 | never",
    ),
    (
        "  9999-12-31T23:00:00 | 9999-12-31T23:00:00+00:00 | UTC@9999-12-31T23:00",
        "  9999-12-31T23:00:00 | never",
    ),
];

/// RFC 3339 as the recorded implementation printed it: the offset is written
/// `+02:00`, rounded to the nearest minute. Only the text is rounded. A zone
/// whose offset has seconds (local mean time, or Monrovia's -00:44:30 until
/// 1972) still fires at the exact instant, which the recorded wall-clock time
/// and claim key beside it pin.
fn rfc3339(instant: &ZonedDateTime) -> String {
    let seconds = instant.offset().seconds();
    let minutes = (seconds.unsigned_abs() + 30) / 60;
    format!(
        "{}{}{:02}:{:02}",
        instant.strftime("%Y-%m-%dT%H:%M:%S"),
        if seconds < 0 { '-' } else { '+' },
        minutes / 60,
        minutes % 60
    )
}

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/scheduler/fixtures/zone_parity.txt")
}

fn local(text: &str) -> DateTime {
    text.parse().unwrap()
}

fn schedule(zone: &str, cron: &str) -> ScheduleConfig {
    ScheduleConfig::new("f.lua", cron, Some(zone), None, Context::new())
        .unwrap_or_else(|error| panic!("{zone} '{cron}': {error}"))
}

/// Every occurrence in `(after, through]`, read through the public, capped
/// entry point the scheduler itself uses.
fn all_due(schedule: &ScheduleConfig, after: DateTime, through: DateTime) -> Vec<DueInstant> {
    let mut all = Vec::new();
    let mut cursor = after;
    loop {
        let (due, truncated) = due_instants(schedule, cursor, through);
        if let Some(last) = due.last() {
            cursor = last.local;
        }
        all.extend(due);
        if !truncated {
            return all;
        }
    }
}

fn line(due: &DueInstant) -> String {
    format!(
        "{} | {} | {}",
        due.local.strftime("%Y-%m-%dT%H:%M:%S"),
        rfc3339(&due.instant),
        due.key
    )
}

/// FNV-1a over the rendered occurrence lines: a compact, stable digest.
fn digest(lines: &[String]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in lines.iter().flat_map(|line| line.bytes().chain(*b"\n")) {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

fn render() -> String {
    let mut out = String::from(
        "# Scheduler occurrences recorded from the chrono-tz 0.10.4 implementation\n\
         # (IANA 2025b) before IF-231 stage 1, then recorded again under IANA 2026f,\n\
         # which changed only America/Havana in 2026 and 2027 (daylight time is\n\
         # kept through that winter). See tests/test_schedule_zone_parity.rs.\n\
         #   exact:   matched wall clock | fired instant | claim key\n\
         #   resolve: wall clock | fired instant | claim key\n\
         #   local:   instant | wall clock\n\
         #   sweep:   year:occurrences:digest\n",
    );
    for (zone, cron, after, through) in EXACT {
        writeln!(out, "exact | {zone} | {cron} | {after} | {through}").unwrap();
        for due in all_due(&schedule(zone, cron), local(after), local(through)) {
            writeln!(out, "  {}", line(&due)).unwrap();
        }
    }
    for (zone, first, step, count) in RESOLVE {
        writeln!(out, "resolve | {zone} | {first} | {step} | {count}").unwrap();
        let schedule = schedule(zone, "0 0 * * *");
        for index in 0..*count {
            let local = local(first) + SignedDuration::from_mins(index * step);
            let shown = local.strftime("%Y-%m-%dT%H:%M:%S");
            match resolve_local(schedule.timezone(), local) {
                Some(instant) => {
                    let key = claim_key(schedule.timezone(), instant.datetime());
                    writeln!(out, "  {shown} | {} | {key}", rfc3339(&instant)).unwrap();
                }
                None => writeln!(out, "  {shown} | never").unwrap(),
            }
        }
    }
    for (zone, first, step, count) in LOCAL_TIME {
        writeln!(out, "local | {zone} | {first} | {step} | {count}").unwrap();
        let schedule = schedule(zone, "0 0 * * *");
        for index in 0..*count {
            let wall_clock = local(first) + SignedDuration::from_mins(index * step);
            let instant = jiff::tz::Offset::UTC.to_timestamp(wall_clock).unwrap();
            let shown = schedule.timezone().local_time(instant);
            // The recording shows a UTC instant with `+00:00` and a wall-clock
            // time with a space, as the recorded implementation printed them.
            writeln!(
                out,
                "  {}+00:00 | {}",
                instant.strftime("%Y-%m-%dT%H:%M:%S"),
                shown.strftime("%Y-%m-%d %H:%M:%S")
            )
            .unwrap();
        }
    }
    for zone in SWEEP_ZONES {
        for cron in SWEEP_EXPRESSIONS {
            let schedule = schedule(zone, cron);
            write!(out, "sweep | {zone} | {cron} |").unwrap();
            for year in SWEEP_YEARS {
                let after = local(&format!("{year}-01-01 00:00"));
                let through = local(&format!("{}-01-01 00:00", year + 1));
                let lines: Vec<String> = all_due(&schedule, after, through)
                    .iter()
                    .map(line)
                    .collect();
                write!(out, " {year}:{}:{:016x}", lines.len(), digest(&lines)).unwrap();
            }
            out.push('\n');
        }
    }
    out
}

#[test]
fn occurrences_and_claim_keys_match_the_recorded_implementation() {
    let actual = render();
    let path = fixture_path();
    if std::env::var_os("IRONFLOW_RECORD_ZONE_PARITY").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &actual).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap();
    let mut differences = Vec::new();
    let mut accepted = 0;
    let mut heading = "";
    for (expected, actual) in expected.lines().zip(actual.lines()) {
        if !expected.starts_with(' ') {
            heading = expected;
        }
        if expected == actual {
            continue;
        }
        if BEYOND_THE_LAST_INSTANT.contains(&(expected, actual)) {
            accepted += 1;
            continue;
        }
        if expected.starts_with("sweep") {
            // Name the years that differ, not two lines of digests.
            let years: Vec<String> = expected
                .split(' ')
                .zip(actual.split(' '))
                .filter(|(expected, actual)| expected != actual)
                .map(|(expected, actual)| format!("{expected} -> {actual}"))
                .collect();
            let sweep: Vec<&str> = expected.splitn(4, " | ").take(3).collect();
            differences.push(format!("[{}]\n  {}", sweep.join(" | "), years.join("\n  ")));
        } else {
            differences.push(format!(
                "[{heading}]\n  recorded: {expected}\n  now:      {actual}"
            ));
        }
    }
    assert!(
        differences.is_empty(),
        "{} line(s) differ from the recording:\n{}",
        differences.len(),
        differences.join("\n")
    );
    assert_eq!(
        expected.lines().count(),
        actual.lines().count(),
        "the number of recorded occurrences changed"
    );
    assert_eq!(
        accepted,
        BEYOND_THE_LAST_INSTANT.len(),
        "an accepted difference from the recording no longer occurs; remove it"
    );
}
