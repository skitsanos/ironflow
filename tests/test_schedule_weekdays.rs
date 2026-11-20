//! IF-180: numeric cron weekdays use the standard numbering, where 0 and 7 are
//! Sunday and 1 to 6 are Monday to Saturday.
//!
//! Every test pins the clock and asserts the calendar days that fire. The
//! reference fortnight runs from Sunday 2026-11-08 to Saturday 2026-11-21;
//! Thursday 2026-11-12 is the day the defect was reproduced on a live server.

use ironflow::scheduler::config::parse_cron;
use jiff::civil::DateTime;

/// A wall-clock time: a parsed expression has no time zone of its own.
fn utc(year: i16, month: i8, day: i8, hour: i8, minute: i8) -> DateTime {
    jiff::civil::datetime(year, month, day, hour, minute, 0, 0)
}

/// Days on which `expression` fires in `(after, through]`, as weekday and date.
fn fired_days(expression: &str, after: DateTime, through: DateTime) -> Vec<String> {
    let schedule = parse_cron(expression)
        .unwrap_or_else(|error| panic!("'{expression}' should be accepted: {error}"));
    schedule
        .after(after)
        .take_while(|instant| *instant <= through)
        .map(|instant| instant.strftime("%a %Y-%m-%d").to_string())
        .collect()
}

/// Days on which `expression` fires in the reference fortnight.
fn fortnight(expression: &str) -> Vec<String> {
    fired_days(
        expression,
        utc(2026, 11, 7, 12, 0),
        utc(2026, 11, 21, 23, 59),
    )
}

#[test]
fn one_to_five_is_monday_to_friday() {
    assert_eq!(
        fortnight("0 9 * * 1-5"),
        [
            "Mon 2026-11-09",
            "Tue 2026-11-10",
            "Wed 2026-11-11",
            "Thu 2026-11-12",
            "Fri 2026-11-13",
            "Mon 2026-11-16",
            "Tue 2026-11-17",
            "Wed 2026-11-18",
            "Thu 2026-11-19",
            "Fri 2026-11-20",
        ]
    );
}

#[test]
fn zero_is_sunday() {
    assert_eq!(fortnight("0 0 * * 0"), ["Sun 2026-11-08", "Sun 2026-11-15"]);
}

#[test]
fn seven_is_also_sunday() {
    assert_eq!(fortnight("0 0 * * 7"), ["Sun 2026-11-08", "Sun 2026-11-15"]);
}

#[test]
fn a_list_of_six_and_zero_is_the_weekend() {
    assert_eq!(
        fortnight("0 9 * * 6,0"),
        [
            "Sun 2026-11-08",
            "Sat 2026-11-14",
            "Sun 2026-11-15",
            "Sat 2026-11-21",
        ]
    );
}

#[test]
fn a_step_over_every_weekday_starts_on_sunday() {
    assert_eq!(
        fortnight("0 9 * * */2"),
        [
            "Sun 2026-11-08",
            "Tue 2026-11-10",
            "Thu 2026-11-12",
            "Sat 2026-11-14",
            "Sun 2026-11-15",
            "Tue 2026-11-17",
            "Thu 2026-11-19",
            "Sat 2026-11-21",
        ]
    );
    assert_eq!(
        fortnight("0 9 * * */3"),
        [
            "Sun 2026-11-08",
            "Wed 2026-11-11",
            "Sat 2026-11-14",
            "Sun 2026-11-15",
            "Wed 2026-11-18",
            "Sat 2026-11-21",
        ]
    );
}

#[test]
fn a_range_may_end_on_sunday_as_seven() {
    assert_eq!(
        fortnight("0 9 * * 5-7"),
        [
            "Sun 2026-11-08",
            "Fri 2026-11-13",
            "Sat 2026-11-14",
            "Sun 2026-11-15",
            "Fri 2026-11-20",
            "Sat 2026-11-21",
        ]
    );
    assert_eq!(fortnight("0 9 * * 0-7").len(), 14, "0-7 is every day");
}

#[test]
fn a_stepped_range_counts_from_its_first_day() {
    let monday_wednesday_friday = [
        "Mon 2026-11-09",
        "Wed 2026-11-11",
        "Fri 2026-11-13",
        "Mon 2026-11-16",
        "Wed 2026-11-18",
        "Fri 2026-11-20",
    ];
    assert_eq!(fortnight("0 9 * * 1-5/2"), monday_wednesday_friday);
    assert_eq!(fortnight("0 9 * * 1-6/2"), monday_wednesday_friday);
}

#[test]
fn a_step_after_one_number_runs_through_seven() {
    // `N/S` is `N-7/S`: 1, 3, 5 and 7.
    assert_eq!(
        fortnight("0 9 * * 1/2"),
        [
            "Sun 2026-11-08",
            "Mon 2026-11-09",
            "Wed 2026-11-11",
            "Fri 2026-11-13",
            "Sun 2026-11-15",
            "Mon 2026-11-16",
            "Wed 2026-11-18",
            "Fri 2026-11-20",
        ]
    );
}

#[test]
fn each_number_names_one_weekday() {
    let expected = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
    for (number, weekday) in expected.iter().enumerate() {
        let fired = fortnight(&format!("0 9 * * {number}"));
        assert_eq!(fired.len(), 2, "weekday {number} fires once a week");
        for day in &fired {
            assert!(
                day.starts_with(weekday),
                "weekday {number} should be {weekday}, but fired on {day}"
            );
        }
    }
}

#[test]
fn the_live_reproduction_fires_on_thursday_for_four_and_not_for_five() {
    // 2026-11-12 is a Thursday. Before the fix `* * * * 5` started runs on
    // it and `* * * * 4` never fired.
    let thursday_morning = utc(2026, 11, 12, 10, 0);

    let four = parse_cron("* * * * 4").unwrap();
    assert_eq!(
        four.after(thursday_morning).next().unwrap(),
        utc(2026, 11, 12, 10, 1)
    );

    let five = parse_cron("* * * * 5").unwrap();
    assert_eq!(
        five.after(thursday_morning).next().unwrap(),
        utc(2026, 11, 13, 0, 0)
    );
}

#[test]
fn day_of_month_and_a_numeric_weekday_fire_on_either() {
    // The first of the month or any Friday. 2026-12-01 is a Tuesday.
    assert_eq!(
        fired_days("0 4 1 * 5", utc(2026, 11, 25, 0, 0), utc(2026, 12, 5, 0, 0)),
        ["Fri 2026-11-27", "Tue 2026-12-01", "Fri 2026-12-04"]
    );
    // 2027-01-01 is a Friday: one date matching both fields fires once.
    assert_eq!(
        fired_days("0 4 1 * 5", utc(2026, 12, 31, 0, 0), utc(2027, 1, 9, 0, 0)),
        ["Fri 2027-01-01", "Fri 2027-01-08"]
    );
    // The union also holds for a weekday list that includes Sunday as 0.
    assert_eq!(
        fired_days(
            "0 4 15 * 6,0",
            utc(2026, 11, 13, 12, 0),
            utc(2026, 11, 22, 23, 59)
        ),
        [
            "Sat 2026-11-14",
            "Sun 2026-11-15",
            "Sat 2026-11-21",
            "Sun 2026-11-22",
        ]
    );
    assert_eq!(
        fired_days(
            "0 4 17 * 6,0",
            utc(2026, 11, 13, 12, 0),
            utc(2026, 11, 22, 23, 59)
        ),
        [
            "Sat 2026-11-14",
            "Sun 2026-11-15",
            "Tue 2026-11-17",
            "Sat 2026-11-21",
            "Sun 2026-11-22",
        ]
    );
}

#[test]
fn weekday_names_keep_their_meaning() {
    assert_eq!(fortnight("0 9 * * Mon-Fri"), fortnight("0 9 * * 1-5"));
    assert_eq!(
        fortnight("0 9 * * sun"),
        ["Sun 2026-11-08", "Sun 2026-11-15"]
    );
    assert_eq!(fortnight("0 9 * * SAT,Sunday"), fortnight("0 9 * * 6,0"));
    assert_eq!(fortnight("0 9 * * Mon-Fri/2"), fortnight("0 9 * * 1-5/2"));
    assert_eq!(fortnight("0 9 * * Sun-Sat").len(), 14);
    // Names and numbers may share a list.
    assert_eq!(
        fortnight("0 9 * * Mon,3,Fri"),
        [
            "Mon 2026-11-09",
            "Wed 2026-11-11",
            "Fri 2026-11-13",
            "Mon 2026-11-16",
            "Wed 2026-11-18",
            "Fri 2026-11-20",
        ]
    );
}

#[test]
fn an_unrestricted_weekday_still_fires_every_day() {
    assert_eq!(fortnight("0 9 * * *").len(), 14);
    assert_eq!(fortnight("0 9 * * ?").len(), 14);
}

#[test]
fn weekdays_outside_zero_to_seven_are_rejected_with_the_numbering() {
    for expression in [
        "0 9 * * 8",
        "0 9 * * 1-8",
        "0 9 * * 9-10",
        "0 9 * * 1,8",
        "0 9 * * 8/2",
        "0 9 1 * 8",
        "0 9 * * 4294967296",
    ] {
        let error = parse_cron(expression).unwrap_err();
        assert!(
            error.contains(expression),
            "message should quote the input: {error}"
        );
        assert!(
            error.contains("weekday") && error.contains("0 to 7"),
            "message for '{expression}' should state the accepted weekdays: {error}"
        );
        assert!(!error.contains('\n'), "message should be one line: {error}");
    }
}

#[test]
fn malformed_weekday_fields_are_rejected_with_a_weekday_message() {
    for expression in [
        "0 9 * * 6-1",
        "0 9 * * 1-5/0",
        "0 9 * * 1-5/8",
        "0 9 * * 1,,2",
        "0 9 * * 1,",
        "0 9 * * ,1",
        "0 9 * * 1-",
        "0 9 * * -1",
        "0 9 * * 1-2-3",
        "0 9 * * 1/",
        "0 9 * * +1",
        "0 9 * * 1*",
        "0 9 * * *5",
        "0 9 * * Mon2030",
        "0 9 * * Mon-5",
        "0 9 * * 1-Fri",
        "0 9 * * */0",
        "0 9 * * */8",
        "0 9 * * Mon-Fri/0",
    ] {
        let error = parse_cron(expression).unwrap_err();
        assert!(
            error.contains(expression),
            "message should quote the input: {error}"
        );
        assert!(
            error.contains("weekday"),
            "message for '{expression}' should name the weekday field: {error}"
        );
        assert!(!error.contains('\n'), "message should be one line: {error}");
    }
}

#[test]
fn unknown_and_misplaced_names_are_still_rejected() {
    for expression in [
        "0 9 * * Funday",
        "0 9 * * Mon/2",
        "0 9 * * Funday-Mon",
        "0 9 * * Sat-Funday",
    ] {
        let error = parse_cron(expression).unwrap_err();
        assert!(
            error.contains(expression),
            "message should quote the input: {error}"
        );
        assert!(!error.contains('\n'), "message should be one line: {error}");
    }
}

mod in_the_schedule_time_zone {
    use ironflow::engine::types::Context;
    use ironflow::scheduler::config::ScheduleConfig;
    use ironflow::scheduler::timing::due_instants;
    use jiff::civil::DateTime;

    fn local(text: &str) -> DateTime {
        text.parse().unwrap()
    }

    #[test]
    fn the_weekday_is_the_local_one() {
        // Monday 09:00 in Auckland is still Sunday in UTC.
        let schedule = ScheduleConfig::new(
            "f.lua",
            "0 9 * * 1",
            Some("Pacific/Auckland"),
            None,
            Context::new(),
        )
        .unwrap();
        let (due, truncated) = due_instants(
            &schedule,
            local("2026-11-07 12:00:00"),
            local("2026-11-14 12:00:00"),
        );

        assert!(!truncated);
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].local, local("2026-11-09 09:00:00"));
        assert_eq!(
            due[0].instant.strftime("%Y-%m-%dT%H:%M:%S%:z").to_string(),
            "2026-11-09T09:00:00+13:00"
        );
    }

    #[test]
    fn a_sunday_schedule_written_as_zero_is_accepted_at_startup() {
        for cron in ["0 0 * * 0", "0 0 * * 7"] {
            let schedule = ScheduleConfig::new("f.lua", cron, None, None, Context::new())
                .unwrap_or_else(|error| panic!("'{cron}' should load: {error}"));
            let (due, _) = due_instants(
                &schedule,
                local("2026-11-12 00:00:00"),
                local("2026-11-16 00:00:00"),
            );
            assert_eq!(due.len(), 1);
            assert_eq!(due[0].local, local("2026-11-15 00:00:00"));
        }
    }

    #[test]
    fn an_out_of_range_weekday_fails_configuration_with_the_numbering() {
        let error = noyalib::from_str::<ScheduleConfig>("flow: f.lua\ncron: \"0 0 * * 8\"\n")
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("weekday") && error.contains("0 to 7"),
            "{error}"
        );
    }
}

// -- IF-217 (e): named ranges across the week boundary ----------------------

#[test]
fn a_named_range_may_cross_the_week_boundary() {
    assert_eq!(fortnight("0 9 * * Sat-Sun"), fortnight("0 9 * * 6-7"));
    assert_eq!(fortnight("0 9 * * Fri-Mon"), fortnight("0 9 * * 5,6,0,1"));
    assert_eq!(
        fortnight("0 9 * * friday-MONDAY"),
        fortnight("0 9 * * 5-7,1")
    );
    assert_eq!(
        fortnight("0 9 * * Mon-Sun").len(),
        14,
        "Mon-Sun is every day"
    );
    assert_eq!(
        fortnight("0 9 * * Sat-Fri").len(),
        14,
        "Sat-Fri is every day"
    );
    // In a list, and beside a restricted day of month (union, as before).
    assert_eq!(fortnight("0 9 * * Wed,Sat-Sun"), fortnight("0 9 * * 3,6,0"));
    assert_eq!(
        fired_days(
            "0 4 17 * Sat-Sun",
            utc(2026, 11, 13, 12, 0),
            utc(2026, 11, 22, 23, 59)
        ),
        fired_days(
            "0 4 17 * 6,0",
            utc(2026, 11, 13, 12, 0),
            utc(2026, 11, 22, 23, 59)
        )
    );
}

#[test]
fn a_stepped_wrapping_named_range_counts_from_its_first_day() {
    // Fri, Sun: every second day of Fri, Sat, Sun, Mon.
    assert_eq!(fortnight("0 9 * * Fri-Mon/2"), fortnight("0 9 * * 5,0"));
    // Sat, Mon, Wed.
    assert_eq!(fortnight("0 9 * * Sat-Wed/2"), fortnight("0 9 * * 6,1,3"));
}

#[test]
fn a_numeric_range_that_runs_backwards_is_still_refused() {
    // A name states the intent; `6-1` may as easily be a typing error.
    let error = parse_cron("0 9 * * 6-1").unwrap_err();
    assert!(error.contains("runs backwards"), "{error}");
}
