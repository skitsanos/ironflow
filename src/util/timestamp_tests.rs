//! The forms recorded from the `chrono` implementation, row by row.

use jiff::Timestamp;

use super::*;

const INSTANTS: &str = include_str!("../../tests/storage/fixtures/timestamps/instants.tsv");

struct Row {
    label: &'static str,
    instant: Timestamp,
    serde: &'static str,
    sql: &'static str,
    event: &'static str,
    micros: i64,
    millis: i64,
}

fn rows() -> Vec<Row> {
    INSTANTS
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
        .map(|line| {
            let field: Vec<&'static str> = line.split('\t').collect();
            Row {
                label: field[0],
                instant: Timestamp::new(field[1].parse().unwrap(), field[2].parse().unwrap())
                    .unwrap(),
                serde: field[3],
                sql: field[4],
                event: field[5],
                micros: field[6].parse().unwrap(),
                millis: field[7].parse().unwrap(),
            }
        })
        .collect()
}

#[test]
fn every_form_matches_the_recording() {
    for row in rows() {
        assert_eq!(rfc3339_z(row.instant), row.serde, "{}", row.label);
        assert_eq!(rfc3339_nanos_z(row.instant), row.sql, "{}", row.label);
        assert_eq!(rfc3339_offset(row.instant), row.event, "{}", row.label);
        assert_eq!(unix_micros(row.instant), row.micros, "{}", row.label);
        assert_eq!(unix_millis(row.instant), row.millis, "{}", row.label);
    }
}

#[test]
fn every_recorded_form_parses_to_its_instant() {
    for row in rows() {
        for text in [row.serde, row.sql, row.event] {
            assert_eq!(parse_rfc3339(text).unwrap(), row.instant, "{text}");
        }
    }
}

/// Why this module exists: `jiff`'s own forms are not the recorded ones.
#[test]
fn the_library_defaults_differ_from_the_recording() {
    let trailing_zero = Timestamp::new(1_767_323_045, 120_000_000).unwrap();
    assert_eq!(trailing_zero.to_string(), "2026-01-02T03:04:05.12Z");
    assert_eq!(rfc3339_z(trailing_zero), "2026-01-02T03:04:05.120Z");

    let before_epoch = Timestamp::new(-2, 999_999_001).unwrap();
    assert_eq!(before_epoch.as_microsecond(), -1_000_000);
    assert_eq!(unix_micros(before_epoch), -1_000_001);
    assert_eq!(before_epoch.as_millisecond(), -1_000);
    assert_eq!(unix_millis(before_epoch), -1_001);
}

#[test]
fn text_forms_sort_like_instants_within_one_form_and_width() {
    // Only the nine-digit form has one width, so only it sorts as text.
    let mut rows: Vec<Row> = rows()
        .into_iter()
        .filter(|row| !row.serde.starts_with('-'))
        .collect();
    rows.sort_by_key(|row| row.instant);
    let text: Vec<String> = rows
        .iter()
        .map(|row| rfc3339_nanos_z(row.instant))
        .collect();
    let mut sorted = text.clone();
    sorted.sort();
    assert_eq!(text, sorted);
}

#[test]
fn microseconds_convert_back_within_the_supported_years() {
    for row in rows() {
        let floor = from_unix_micros(row.micros).unwrap();
        assert_eq!(unix_micros(floor), row.micros, "{}", row.label);
        assert!(floor <= row.instant, "{}", row.label);
    }
    assert_eq!(unix_micros(Timestamp::MIN), -377_705_023_201_000_000);
    assert_eq!(unix_micros(Timestamp::MAX), 253_402_207_200_999_999);
    // Include the fraction of the final supported second in the round trip.
    for micros in [253_402_207_200_000_001, 253_402_207_200_999_999] {
        assert_eq!(from_unix_micros(micros).map(unix_micros), Some(micros));
    }
    assert_eq!(from_unix_micros(253_402_207_201_000_000), None);
    assert_eq!(from_unix_micros(-377_705_023_201_000_001), None);
    assert_eq!(from_unix_micros(i64::MAX), None);
    assert_eq!(from_unix_micros(i64::MIN), None);
}

#[test]
fn the_range_limits_have_text_forms_and_nothing_beyond_them_parses() {
    assert_eq!(rfc3339_z(Timestamp::MIN), "-9999-01-02T01:59:59Z");
    assert_eq!(rfc3339_z(Timestamp::MAX), "9999-12-30T22:00:00.999999999Z");
    for instant in [Timestamp::MIN, Timestamp::MAX] {
        for text in [
            rfc3339_z(instant),
            rfc3339_offset(instant),
            rfc3339_nanos_z(instant),
        ] {
            assert_eq!(parse_rfc3339(&text).unwrap(), instant, "{text}");
        }
    }
    for text in [
        "9999-12-30T22:00:01Z",
        "9999-12-31T23:59:59Z",
        "+10000-01-01T00:00:00Z",
        "-9999-01-02T01:59:58Z",
        "-262143-01-01T00:00:00Z",
    ] {
        assert!(parse_rfc3339(text).is_err(), "{text}");
    }
}

#[test]
fn a_signed_year_is_read_in_both_spellings() {
    let expected = Timestamp::new(-62_198_755_200, 0).unwrap();
    assert_eq!(parse_rfc3339("-0001-01-01T00:00:00Z").unwrap(), expected);
    assert_eq!(parse_rfc3339("-000001-01-01T00:00:00Z").unwrap(), expected);
    for text in ["-", "-0001", "-0001-", "-00a1-01-01T00:00:00Z", "-é001-01"] {
        assert!(parse_rfc3339(text).is_err(), "{text}");
    }
}

#[test]
fn elapsed_time_is_whole_milliseconds_and_never_negative() {
    let at = |seconds, nanos| Timestamp::new(seconds, nanos).unwrap();
    assert_eq!(elapsed_millis(at(10, 0), at(10, 0)), Some(0));
    assert_eq!(elapsed_millis(at(10, 0), at(11, 500_999_999)), Some(1_500));
    assert_eq!(elapsed_millis(at(10, 999_999), at(10, 1_000_000)), Some(0));
    assert_eq!(elapsed_millis(at(10, 1_000_000), at(10, 0)), None);
    assert_eq!(
        elapsed_millis(Timestamp::MIN, Timestamp::MAX),
        Some(631_107_230_401_999)
    );
}

#[test]
fn serde_adapters_write_the_recorded_form_and_read_absent_values() {
    #[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq)]
    struct Record {
        #[serde(with = "super::serde_instant")]
        at: Timestamp,
        #[serde(default, with = "super::serde_option")]
        maybe: Option<Timestamp>,
    }
    let at = Timestamp::new(1_767_323_045, 120_000_000).unwrap();
    let record = Record {
        at,
        maybe: Some(at),
    };
    let text = r#"{"at":"2026-01-02T03:04:05.120Z","maybe":"2026-01-02T03:04:05.120Z"}"#;
    assert_eq!(serde_json::to_string(&record).unwrap(), text);
    assert_eq!(serde_json::from_str::<Record>(text).unwrap(), record);
    // An owned string, as a reader or a flattened record supplies it.
    assert_eq!(
        serde_json::from_reader::<_, Record>(text.as_bytes()).unwrap(),
        record
    );
    assert_eq!(
        serde_json::from_value::<Record>(serde_json::from_str(text).unwrap()).unwrap(),
        record
    );

    let absent = Record { at, maybe: None };
    assert_eq!(
        serde_json::to_string(&absent).unwrap(),
        r#"{"at":"2026-01-02T03:04:05.120Z","maybe":null}"#
    );
    for text in [
        r#"{"at":"2026-01-02T03:04:05.120Z","maybe":null}"#,
        r#"{"at":"2026-01-02T03:04:05.120+00:00"}"#,
    ] {
        assert_eq!(serde_json::from_str::<Record>(text).unwrap(), absent);
    }
    for text in [
        r#"{"at":1767323045}"#,
        r#"{"at":null}"#,
        r#"{"at":"2026-01-02T03:04:05.120Z","maybe":17}"#,
        r#"{"maybe":null}"#,
    ] {
        assert!(serde_json::from_str::<Record>(text).is_err(), "{text}");
    }
}
