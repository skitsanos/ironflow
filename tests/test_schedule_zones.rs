//! IF-231: which time zone names a schedule accepts, and where their rules
//! come from.

use chrono::NaiveDateTime;
use ironflow::engine::types::Context;
use ironflow::scheduler::config::ScheduleConfig;
use ironflow::scheduler::timing::resolve_local;

fn zone(name: &str) -> Result<ScheduleConfig, String> {
    ScheduleConfig::new("f.lua", "0 2 * * *", Some(name), None, Context::new())
}

fn local(text: &str) -> NaiveDateTime {
    NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M").unwrap()
}

/// The parity recording in `tests/scheduler/fixtures/zone_parity.txt` holds for
/// one IANA release. When a `jiff-tzdb` update moves it, this fails first, so a
/// rule change is reviewed as one and not mistaken for a scheduler regression.
#[test]
fn the_bundled_zone_data_is_the_reviewed_iana_release() {
    assert_eq!(
        jiff_tzdb::VERSION,
        "2026f",
        "the bundled IANA release changed: review the zone changes it makes, \
         record the parity fixture again (see tests/test_schedule_zone_parity.rs) \
         and name the new release here and in docs/CLI_REFERENCE.md"
    );
}

#[test]
fn every_zone_name_the_previous_implementation_accepted_still_loads() {
    let names = include_str!("scheduler/fixtures/zone_names.txt");
    let rejected: Vec<&str> = names
        .lines()
        .filter(|name| match zone(name) {
            Ok(schedule) => schedule.timezone().name() != *name,
            Err(_) => true,
        })
        .collect();
    assert!(names.lines().count() > 590);
    assert_eq!(rejected, Vec::<&str>::new());
}

#[test]
fn every_bundled_zone_name_loads_under_its_own_spelling() {
    for name in jiff_tzdb::available() {
        let schedule = zone(name).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(schedule.timezone().name(), name);
    }
}

/// The name is part of every claim key, so a second spelling of one zone would
/// let two replicas claim the same fire.
#[test]
fn a_zone_name_must_be_spelled_exactly() {
    for name in [
        "europe/berlin",
        "EUROPE/BERLIN",
        "Europe/berlin",
        "utc",
        "Utc",
        " Europe/Berlin",
        "Europe/Berlin ",
        "",
        "Etc/Unknown",
        "Europe/Atlantis",
        "+02:00",
        "CET-1CEST,M3.5.0,M10.5.0/3",
        "../Europe/Berlin",
        "/usr/share/zoneinfo/Europe/Berlin",
    ] {
        let error = zone(name).unwrap_err();
        assert_eq!(
            error,
            format!("schedule timezone '{name}' is not a known IANA time zone name")
        );
    }
}

/// The one difference from the previous implementation: its rules stopped at
/// 2100, after which a zone kept its standard offset all year. Zone rules now
/// continue. No schedule reaches these dates, because `cron` enumerates the
/// years 1970 to 2099.
#[test]
fn daylight_saving_rules_continue_after_2100() {
    let berlin = zone("Europe/Berlin").unwrap();
    let resolved = |text: &str| {
        resolve_local(berlin.timezone(), local(text))
            .unwrap()
            .to_rfc3339()
    };
    assert_eq!(resolved("2150-03-29 02:30"), "2150-03-29T03:00:00+02:00");
    assert_eq!(resolved("2150-07-01 02:30"), "2150-07-01T02:30:00+02:00");
    assert_eq!(resolved("2150-10-25 02:30"), "2150-10-25T02:30:00+02:00");
    assert_eq!(resolved("2150-12-01 02:30"), "2150-12-01T02:30:00+01:00");
}
