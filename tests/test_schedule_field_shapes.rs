//! A cron token is exactly one field.
//!
//! The `cron` crate separates fields with optional whitespace and accepts an
//! optional year, so an unchecked token such as `5*/2` was read as two fields
//! and the rest of the expression shifted. These tests pin the clock: the
//! malformed expressions are refused with a message that names the field, and
//! well-formed lists, ranges, steps and month names fire on the same instants
//! as before the check existed.

use chrono::{DateTime, TimeZone as _, Utc};
use ironflow::scheduler::config::parse_cron;

fn utc(year: i32, month: u32, day: u32, hour: u32, minute: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(year, month, day, hour, minute, 0)
        .unwrap()
}

/// Instants at which `expression` fires in `(after, through]`.
fn fired(expression: &str, after: DateTime<Utc>, through: DateTime<Utc>) -> Vec<String> {
    let schedule = parse_cron(expression)
        .unwrap_or_else(|error| panic!("'{expression}' should be accepted: {error}"));
    schedule
        .after(&after)
        .take_while(|instant| *instant <= through)
        .map(|instant| instant.format("%Y-%m-%d %H:%M:%S").to_string())
        .collect()
}

/// Instants on Thursday 2026-11-12, as `HH:MM`.
fn on_the_day(expression: &str) -> Vec<String> {
    fired(
        expression,
        utc(2026, 11, 11, 23, 59),
        utc(2026, 11, 12, 23, 59),
    )
    .into_iter()
    .map(|instant| {
        let (date, time) = instant.split_once(' ').unwrap();
        assert_eq!(date, "2026-11-12");
        time.strip_suffix(":00")
            .expect("schedules fire at second zero")
            .to_string()
    })
    .collect()
}

/// Days of December 2026 on which `expression` fires at midnight.
fn december_days(expression: &str) -> Vec<u32> {
    fired(
        expression,
        utc(2026, 11, 30, 12, 0),
        utc(2026, 12, 31, 23, 59),
    )
    .into_iter()
    .map(|instant| {
        let day = instant
            .strip_prefix("2026-12-")
            .and_then(|rest| rest.strip_suffix(" 00:00:00"))
            .unwrap_or_else(|| panic!("unexpected instant {instant}"));
        day.parse().unwrap()
    })
    .collect()
}

/// Months of 2027 in which `expression` fires at midnight on the first.
fn months_of_2027(expression: &str) -> Vec<u32> {
    fired(
        expression,
        utc(2026, 12, 31, 12, 0),
        utc(2027, 12, 31, 23, 59),
    )
    .into_iter()
    .map(|instant| {
        let month = instant
            .strip_prefix("2027-")
            .and_then(|rest| rest.strip_suffix("-01 00:00:00"))
            .unwrap_or_else(|| panic!("unexpected instant {instant}"));
        month.parse().unwrap()
    })
    .collect()
}

/// Why `expression` is not refused in one line that quotes it and names
/// `field`, if it is not. An accepted expression is reported with the instants
/// it fires on.
fn refusal_problem(field: &str, expression: &str) -> Option<String> {
    match parse_cron(expression) {
        Ok(schedule) => {
            let next = schedule
                .after(&utc(2026, 11, 12, 0, 0))
                .take(3)
                .map(|instant| instant.format("%Y-%m-%d %H:%M:%S").to_string())
                .collect::<Vec<_>>();
            Some(format!("'{expression}' was accepted and fires at {next:?}"))
        }
        Err(error) => {
            let names_the_field = error.contains(&format!("{field} '"))
                || error.contains(&format!("the {field} field"));
            let well_formed = error.contains(expression) && !error.contains('\n');
            (!(names_the_field && well_formed)).then(|| {
                format!(
                    "'{expression}' should be refused in one line that quotes it \
                     and names the {field} field, but the message is: {error}"
                )
            })
        }
    }
}

/// Assert every `(field, expression)` is refused with a message naming the
/// field. Every case is tried before the assertion.
fn assert_each_refused(cases: &[(&str, &str)]) {
    let problems = cases
        .iter()
        .filter_map(|(field, expression)| refusal_problem(field, expression))
        .collect::<Vec<_>>();
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

fn assert_refused(field: &str, expressions: &[&str]) {
    let cases = expressions
        .iter()
        .map(|expression| (field, *expression))
        .collect::<Vec<_>>();
    assert_each_refused(&cases);
}

// -- Shifted readings ------------------------------------------------------

#[test]
fn a_minute_token_holding_two_fields_is_refused() {
    // Read as minute 5 of every second hour, and as every minute of hour 5.
    assert_refused("minute", &["5*/2 * * * *", "*5 * * * *", "** * * * *"]);
}

#[test]
fn an_hour_token_holding_two_fields_is_refused() {
    // Read as hour 2 on every fifth day of the month.
    assert_refused("hour", &["0 2*/5 * * *", "0 2* * * *"]);
}

#[test]
fn a_day_token_holding_two_fields_is_refused() {
    assert_refused("day of month", &["0 0 1* * *", "0 0 ?* * *", "0 0 ?? * *"]);
}

#[test]
fn a_month_token_holding_two_fields_is_refused() {
    // Read as 02:00 daily in January, the last token becoming the year; with
    // `*/2` there, in even years only.
    assert_refused(
        "month",
        &["0 2 * 1* *", "0 2 * 1* */2", "0 0 * Jan* *", "0 0 1 1*/2 *"],
    );
}

#[test]
fn a_token_that_joins_its_neighbour_is_refused() {
    // The crate also skips whitespace inside a range, list or step, so a token
    // that starts or ends with `-`, `,` or `/` joined the field beside it:
    // `1 -5 1* * *` was minutes 1 to 5 of hour 1. The field named is the one
    // holding the first malformed token.
    assert_each_refused(&[
        ("minute", "1, 5 1* * *"),
        ("minute", "*/ 5 1* * *"),
        ("hour", "1 -5 1* * *"),
        ("day of month", "0 1 -5 1* *"),
        ("day of month", "0 1 ,5 1* *"),
    ]);
}

#[test]
fn a_first_token_that_joins_the_seconds_field_is_refused() {
    // Seconds are fixed at zero. A leading `/5` turned them into `0/5`, which
    // fired every five seconds of minute 1.
    assert_refused("minute", &["/5 1* * * *", "-5 1* * * *", ",30 1* * * *"]);
}

// -- Malformed tokens name their field -------------------------------------

#[test]
fn malformed_tokens_are_refused_with_the_field_they_are_in() {
    assert_refused(
        "minute",
        &[
            "Jan * * * *",
            "? * * * *",
            "1- * * * *",
            "1-2-3 * * * *",
            "*/ * * * *",
            "*/x * * * *",
            "1/2/3 * * * *",
            "+1 * * * *",
            "5x * * * *",
            "１ * * * *",
        ],
    );
    assert_refused("hour", &["0 ? * * *", "0 noon * * *", "0 1-x * * *"]);
    assert_refused("day of month", &["0 0 Mon * *", "0 0 1st * *"]);
    assert_refused(
        "month",
        &[
            "0 0 * ? *",
            "0 0 * Jan/2 *",
            "0 0 * 1-Jun *",
            "0 0 * Jan-3 *",
            "0 0 * Jan2030 *",
            "0 0 * Jan-Mar-May *",
        ],
    );
}

#[test]
fn an_empty_list_item_is_refused_with_its_field() {
    for (expression, field) in [
        ("1, * * * *", "minute"),
        (",1 * * * *", "minute"),
        ("1,,2 * * * *", "minute"),
        ("0 1,,2 * * *", "hour"),
        ("0 0 , * *", "day of month"),
        ("0 0 * Jan, *", "month"),
    ] {
        let error = parse_cron(expression).unwrap_err();
        assert!(error.contains(expression), "{error}");
        assert!(
            error.contains(&format!("the {field} field has an empty list item")),
            "{expression}: {error}"
        );
        assert!(!error.contains('\n'), "{error}");
    }
}

#[test]
fn the_first_malformed_field_is_the_one_reported() {
    let error = parse_cron("5*/2 * * 1* 8").unwrap_err();
    assert!(error.contains("minute '5*/2'"), "{error}");
    let error = parse_cron("0 2 * 1* 8").unwrap_err();
    assert!(error.contains("month '1*'"), "{error}");
}

// -- Well-formed expressions keep their instants ---------------------------

#[test]
fn minute_lists_ranges_and_steps_fire_as_before() {
    assert_eq!(
        on_the_day("0,15,30,45 9 * * *"),
        ["09:00", "09:15", "09:30", "09:45"]
    );
    assert_eq!(
        on_the_day("*/15 9 * * *"),
        ["09:00", "09:15", "09:30", "09:45"]
    );
    assert_eq!(on_the_day("58-59 9 * * *"), ["09:58", "09:59"]);
    assert_eq!(
        on_the_day("10-40/10 9 * * *"),
        ["09:10", "09:20", "09:30", "09:40"]
    );
    // `N/S` starts at `N` and runs to the end of the hour.
    assert_eq!(on_the_day("5/20 9 * * *"), ["09:05", "09:25", "09:45"]);
    assert_eq!(
        on_the_day("0,10-12,*/30 9 * * *"),
        ["09:00", "09:10", "09:11", "09:12", "09:30"]
    );
}

#[test]
fn hour_lists_ranges_and_steps_fire_as_before() {
    assert_eq!(on_the_day("0 9-11 * * *"), ["09:00", "10:00", "11:00"]);
    assert_eq!(
        on_the_day("0 */6 * * *"),
        ["00:00", "06:00", "12:00", "18:00"]
    );
    assert_eq!(
        on_the_day("30 0,12-14 * * *"),
        ["00:30", "12:30", "13:30", "14:30"]
    );
    assert_eq!(on_the_day("0 8-20/6 * * *"), ["08:00", "14:00", "20:00"]);
    assert_eq!(on_the_day("0 20/2 * * *"), ["20:00", "22:00"]);
    // Leading zeros are the same numbers.
    assert_eq!(on_the_day("05 09 * * *"), ["09:05"]);
}

#[test]
fn day_of_month_lists_ranges_steps_and_the_question_mark_fire_as_before() {
    assert_eq!(december_days("0 0 1,15 * *"), [1, 15]);
    assert_eq!(december_days("0 0 10-12 * *"), [10, 11, 12]);
    assert_eq!(december_days("0 0 */10 * *"), [1, 11, 21, 31]);
    assert_eq!(december_days("0 0 1-7/3 * *"), [1, 4, 7]);
    assert_eq!(december_days("0 0 28/2 * *"), [28, 30]);
    assert_eq!(december_days("0 0 ? * *").len(), 31);
    assert_eq!(december_days("0 0 ?/10 * *"), [1, 11, 21, 31]);
    assert_eq!(december_days("0 0 1,20-21,*/15 * *"), [1, 16, 20, 21, 31]);
}

#[test]
fn month_numbers_and_names_fire_as_before() {
    assert_eq!(months_of_2027("0 0 1 3-5 *"), [3, 4, 5]);
    assert_eq!(months_of_2027("0 0 1 */3 *"), [1, 4, 7, 10]);
    assert_eq!(months_of_2027("0 0 1 6/3 *"), [6, 9, 12]);
    assert_eq!(months_of_2027("0 0 1 2-10/4 *"), [2, 6, 10]);
    assert_eq!(months_of_2027("0 0 1 Jan,Jul *"), [1, 7]);
    assert_eq!(months_of_2027("0 0 1 jan-mar *"), [1, 2, 3]);
    assert_eq!(months_of_2027("0 0 1 Feb-Jun/2 *"), [2, 4, 6]);
    assert_eq!(months_of_2027("0 0 1 MARCH *"), [3]);
    assert_eq!(
        months_of_2027("0 0 1 September-december *"),
        [9, 10, 11, 12]
    );
    // Names and numbers may share a list.
    assert_eq!(months_of_2027("0 0 1 1,Jul,10-11 *"), [1, 7, 10, 11]);
}

#[test]
fn a_restricted_day_and_weekday_still_fire_on_either() {
    // The first, the fifteenth, or any Monday. 2026-12-01 is a Tuesday.
    assert_eq!(
        fired(
            "0 4 1,15 * Mon",
            utc(2026, 11, 30, 12, 0),
            utc(2026, 12, 16, 0, 0)
        ),
        [
            "2026-12-01 04:00:00",
            "2026-12-07 04:00:00",
            "2026-12-14 04:00:00",
            "2026-12-15 04:00:00",
        ]
    );
    // A stepped day of month is restricted too: every tenth day or a Monday,
    // in January only. 2027-01-04 is a Monday.
    assert_eq!(
        fired(
            "0 4 */10 Jan 1",
            utc(2026, 12, 31, 12, 0),
            utc(2027, 1, 12, 0, 0)
        ),
        [
            "2027-01-01 04:00:00",
            "2027-01-04 04:00:00",
            "2027-01-11 04:00:00",
        ]
    );
}

#[test]
fn values_outside_a_field_are_still_refused() {
    // The shape is fine, so the value check stays with the parser.
    for expression in [
        "60 * * * *",
        "0 24 * * *",
        "0 0 0 * *",
        "0 0 32 * *",
        "0 0 * 0 *",
        "0 0 * 13 *",
        "0 0 * Smarch *",
        "0 0 * Mar-Jan *",
        "*/0 * * * *",
        "0 */24 * * *",
        "30-10 * * * *",
        "99999999999999999999 * * * *",
    ] {
        let error = parse_cron(expression).unwrap_err();
        assert!(
            error.contains(expression),
            "message should quote the input: {error}"
        );
        assert!(!error.contains('\n'), "message should be one line: {error}");
    }
}

mod at_startup {
    use ironflow::engine::types::Context;
    use ironflow::scheduler::config::ScheduleConfig;

    #[test]
    fn a_shifted_expression_fails_schedule_configuration() {
        for (cron, field) in [
            ("5*/2 * * * *", "minute '5*/2'"),
            ("0 2 * 1* *", "month '1*'"),
        ] {
            let error = ScheduleConfig::new("f.lua", cron, None, None, Context::new()).unwrap_err();
            assert!(error.contains(cron) && error.contains(field), "{error}");
        }
    }

    #[test]
    fn a_shifted_expression_fails_yaml_configuration_with_the_field() {
        let error = noyalib::from_str::<ScheduleConfig>("flow: f.lua\ncron: \"0 2 * 1* *\"\n")
            .unwrap_err()
            .to_string();
        assert!(error.contains("month '1*'"), "{error}");
    }
}
