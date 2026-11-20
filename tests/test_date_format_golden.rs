//! Golden record of the `date_format` node (IF-231, stage 4).
//!
//! `tests/nodes/fixtures/date_format_golden.jsonl` was recorded from the
//! `chrono` implementation before the node moved to `jiff`. Each line is a
//! case and what the node returned for it: the formatted text and Unix
//! seconds, or the error text. The cases are built here; the fixture holds
//! only what was observed. Intended differences are the year range, the
//! century of a year before 0000, malformed output timezone offsets, and
//! date-time errors no longer hidden by the date-only fallback.
//!
//! To record again after an intended change, run
//! `cargo test --test nodes test_date_format_golden::record -- --ignored`
//! and review the fixture diff line by line.

use std::path::PathBuf;

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;
use serde_json::{Value, json};

/// Instants given as RFC 3339 text, which the node auto-detects.
const INSTANTS: [&str; 14] = [
    "2024-06-05T09:07:03.250Z",
    "1969-12-31T23:59:59.999999999Z",
    "1970-01-01T00:00:00Z",
    "2000-02-29T00:00:00+00:00",
    "2021-01-03T12:00:00-08:00",
    "2024-12-30T23:59:59.000001+05:30",
    "1900-03-01T00:30:00Z",
    "0000-01-01T00:00:00Z",
    "0001-01-01T00:00:00.5Z",
    "0099-12-31T11:59:59Z",
    "9999-12-31T23:59:59Z",
    "2016-12-31T23:59:60Z",
    "2100-02-28T13:00:00.123456Z",
    "2023-01-01T12:00:00.5Z",
];

/// Instants RFC 3339 cannot spell, read with `%Y-%m-%d %H:%M:%S`.
const WIDE_YEARS: [&str; 7] = [
    "-0001-12-31 10:00:00",
    "-9999-01-01 00:00:00",
    "-10000-12-31 23:59:59",
    "+10000-01-01 00:00:00",
    "+12345-06-07 01:02:03",
    "+262142-12-31 23:59:59",
    "-262143-01-01 00:00:00",
];

const ZONES: [Option<&str>; 6] = [
    None,
    Some("UTC"),
    Some("+05:30"),
    Some("-08:00"),
    Some("+14:00"),
    Some("-00:01"),
];

/// Every specifier that formats, in one string.
const ALL_SPECIFIERS: &str = "%a|%A|%b|%B|%h|%C|%y|%Y|%q|%m|%d|%e|%j|%U|%W|%G|%g|%V|%u|%w|%H|%k|%I|%l|%M|%S|%p|%P|%f|%.f|%.3f|%.6f|%.9f|%3f|%6f|%9f|%z|%:z|%::z|%:::z|%Z|%s|%+|%c|%x|%X|%r|%D|%F|%T|%R|%v|%%|%n|%t";

/// The specifiers that take a padding modifier.
const NUMERIC: &str = "CyYqmdejUWGgVuwHkIlMSfs";

/// Specifiers a formatted value is read back with, after a full date and time.
const READ_BACK: [&str; 40] = [
    "%a",
    "%A",
    "%b",
    "%B",
    "%h",
    "%C",
    "%y",
    "%q",
    "%j",
    "%U",
    "%W",
    "%G",
    "%g",
    "%V",
    "%u",
    "%w",
    "%k",
    "%I",
    "%l",
    "%p",
    "%P",
    "%f",
    "%.f",
    "%.3f",
    "%.6f",
    "%.9f",
    "%3f",
    "%6f",
    "%9f",
    "%z",
    "%:z",
    "%::z",
    "%:::z",
    "%Z",
    "%s",
    "%+",
    "%c",
    "%x %X",
    "%r",
    "%D %F %T %R %v",
];

const PREFIXES: [&str; 17] = [
    "", "-", "_", "0", "#", ":", "::", ":::", ".", ".3", ".6", ".9", "3", "6", "9", "^", "4",
];

fn format_case(input: &str, input_format: Option<&str>, zone: Option<&str>, out: &str) -> Value {
    let mut config = json!({ "input": input, "output_format": out });
    if let Some(input_format) = input_format {
        config["input_format"] = json!(input_format);
    }
    if let Some(zone) = zone {
        config["timezone"] = json!(zone);
    }
    json!({ "config": config })
}

/// Parse `input` with `format` and print everything the result holds.
fn parse_case(input: &str, format: &str) -> Value {
    json!({ "config": {
        "input": input,
        "input_format": format,
        "output_format": "%Y-%m-%dT%H:%M:%S%.f%:z"
    }})
}

fn auto_case(input: &str) -> Value {
    json!({ "config": { "input": input, "output_format": "%Y-%m-%dT%H:%M:%S%.f%:z" } })
}

fn output_cases(cases: &mut Vec<Value>) {
    let padded: String = ["-", "_", "0"]
        .iter()
        .flat_map(|pad| NUMERIC.chars().map(move |spec| format!("%{pad}{spec}|")))
        .collect();
    for zone in ZONES {
        for instant in INSTANTS {
            cases.push(format_case(instant, None, zone, ALL_SPECIFIERS));
            cases.push(format_case(instant, None, zone, &padded));
        }
        for instant in WIDE_YEARS {
            let read = Some("%Y-%m-%d %H:%M:%S");
            cases.push(format_case(instant, read, zone, ALL_SPECIFIERS));
            cases.push(format_case(instant, read, zone, &padded));
        }
    }
    // Every printable ASCII character after every modifier, valid or not.
    for prefix in PREFIXES {
        for spec in ' '..='~' {
            let out = format!("%{prefix}{spec}");
            cases.push(format_case(
                "2024-06-05T09:07:03.250049-03:30",
                None,
                None,
                &out,
            ));
        }
    }
    for out in [
        "",
        "%",
        "%%%",
        "100%% é ü 日本 %Y 😽",
        "%é",
        "%-",
        "%:",
        "%::",
        "%.",
        "%.3",
        "%-D",
        "%_F",
        "%0T",
        "%-c",
        "%-%",
        "%-n",
        "%--d",
        "%-_d",
        "%Y%m%d%H%M%S",
        "  %Y\t%m\n%d  ",
        "%:::::z",
        "%#Z",
        "%.3fx%.6fx%.9f",
    ] {
        cases.push(format_case(
            "2024-06-05T09:07:03.250049-03:30",
            None,
            None,
            out,
        ));
    }
}

fn read_back_cases(cases: &mut Vec<Value>) {
    for instant in [
        INSTANTS[0],
        INSTANTS[1],
        INSTANTS[4],
        INSTANTS[5],
        INSTANTS[7],
        INSTANTS[11],
    ] {
        for spec in READ_BACK {
            cases.push(json!({ "read_back": { "input": instant, "spec": spec } }));
        }
    }
}

const DATE_PARSE: &[(&str, &str)] = &[
    // Numeric widths, signs and padding.
    ("2024-06-15", "%Y-%m-%d"),
    ("2024-6-5", "%Y-%m-%d"),
    ("2024- 6- 5", "%Y-%m-%d"),
    ("2024-006-15", "%Y-%m-%d"),
    ("24-06-15", "%Y-%m-%d"),
    ("5-06-15", "%Y-%m-%d"),
    ("20240615", "%Y%m%d"),
    ("202406151030", "%Y%m%d%H%M"),
    ("12345-06-15", "%Y-%m-%d"),
    ("+12345-06-15", "%Y-%m-%d"),
    ("+2024-06-15", "%Y-%m-%d"),
    ("-0044-03-15", "%Y-%m-%d"),
    ("-44-03-15", "%Y-%m-%d"),
    ("0000-01-01", "%Y-%m-%d"),
    ("9999-12-31", "%Y-%m-%d"),
    ("+10000-01-01", "%Y-%m-%d"),
    ("-9999-01-01", "%Y-%m-%d"),
    ("-10000-01-01", "%Y-%m-%d"),
    ("+262142-12-31", "%Y-%m-%d"),
    ("+262143-01-01", "%Y-%m-%d"),
    ("-262143-01-01", "%Y-%m-%d"),
    ("-262144-01-01", "%Y-%m-%d"),
    ("+99999999999999999999-01-01", "%Y-%m-%d"),
    ("+3000000000-01-01", "%Y-%m-%d"),
    ("2024-02-29", "%Y-%m-%d"),
    ("2023-02-29", "%Y-%m-%d"),
    ("1900-02-29", "%Y-%m-%d"),
    ("2000-02-29", "%Y-%m-%d"),
    ("2024-04-31", "%Y-%m-%d"),
    ("2024-00-10", "%Y-%m-%d"),
    ("2024-13-10", "%Y-%m-%d"),
    ("2024-06-00", "%Y-%m-%d"),
    ("2024-06-32", "%Y-%m-%d"),
    ("2024-06-15", "%-Y-%-m-%-d"),
    ("2024-06-15", "%_Y-%_m-%_d"),
    ("2024-06-15", "%0Y-%0m-%0d"),
    (" 5/ 6/2024", "%e/%m/%Y"),
    ("5/6/2024", "%e/%m/%Y"),
    // Two-digit years and centuries.
    ("69-01-01", "%y-%m-%d"),
    ("70-01-01", "%y-%m-%d"),
    ("00-01-01", "%y-%m-%d"),
    ("99-12-31", "%y-%m-%d"),
    ("9-12-31", "%y-%m-%d"),
    ("20 24-06-15", "%C %y-%m-%d"),
    ("19 99-06-15", "%C %y-%m-%d"),
    ("20-06-15", "%C-%m-%d"),
    ("2024 20 24-06-15", "%Y %C %y-%m-%d"),
    ("2024 19 24-06-15", "%Y %C %y-%m-%d"),
    ("2024 25-06-15", "%Y %y-%m-%d"),
    ("-0044 44-03-15", "%Y %y-%m-%d"),
    // Month and weekday names.
    ("15 Jun 2024", "%d %b %Y"),
    ("15 jun 2024", "%d %b %Y"),
    ("15 JUN 2024", "%d %h %Y"),
    ("15 June 2024", "%d %b %Y"),
    ("15 June 2024", "%d %B %Y"),
    ("15 Jun 2024", "%d %B %Y"),
    ("15 JUNE 2024", "%d %B %Y"),
    ("15 Sept 2024", "%d %B %Y"),
    ("15 September 2024", "%d %B %Y"),
    ("15 Ju 2024", "%d %b %Y"),
    ("15 Foo 2024", "%d %b %Y"),
    ("15 May 2024", "%d %B %Y"),
    ("Sat 15 Jun 2024", "%a %d %b %Y"),
    ("Saturday 15 Jun 2024", "%A %d %b %Y"),
    ("sat 15 Jun 2024", "%A %d %b %Y"),
    ("Saturday 15 Jun 2024", "%a %d %b %Y"),
    ("Sun 15 Jun 2024", "%a %d %b %Y"),
    ("Wednesday 5 Jun 2024", "%A %d %b %Y"),
    ("Xyz 15 Jun 2024", "%a %d %b %Y"),
    ("6 15 Jun 2024", "%w %d %b %Y"),
    ("6 15 Jun 2024", "%u %d %b %Y"),
    ("0 16 Jun 2024", "%w %d %b %Y"),
    ("7 16 Jun 2024", "%u %d %b %Y"),
    ("7 16 Jun 2024", "%w %d %b %Y"),
    ("0 16 Jun 2024", "%u %d %b %Y"),
    ("8 16 Jun 2024", "%u %d %b %Y"),
    // Day of year, week numbers, ISO week dates, quarters.
    ("2024 167", "%Y %j"),
    ("2024 1", "%Y %j"),
    ("2024 366", "%Y %j"),
    ("2023 366", "%Y %j"),
    ("2024 367", "%Y %j"),
    ("2024 000", "%Y %j"),
    ("2024 167 06", "%Y %j %m"),
    ("2024 167 07", "%Y %j %m"),
    ("2024 23 6", "%Y %U %w"),
    ("2024 24 6", "%Y %W %u"),
    ("2024 00 1", "%Y %U %w"),
    ("2024 00 0", "%Y %U %w"),
    ("2024 00 1", "%Y %W %u"),
    ("2023 00 0", "%Y %W %w"),
    ("2024 53 1", "%Y %U %w"),
    ("2024 54 1", "%Y %U %w"),
    ("2024 23", "%Y %U"),
    ("2024 23 Sat", "%Y %U %a"),
    ("2024 23 Sat 24", "%Y %U %a %W"),
    ("2024 23 Sat 23", "%Y %U %a %W"),
    ("2024-W24-6", "%G-W%V-%u"),
    ("2020-W53-7", "%G-W%V-%u"),
    ("2021-W53-7", "%G-W%V-%u"),
    ("2024-W00-1", "%G-W%V-%u"),
    ("2025-W01-1", "%G-W%V-%u"),
    ("24-W24-6", "%g-W%V-%u"),
    ("2024-W24", "%G-W%V"),
    ("2024-W24-6 2024", "%G-W%V-%u %Y"),
    ("2025-W01-1 2025", "%G-W%V-%u %Y"),
    ("2024-06-15 Q2", "%Y-%m-%d Q%q"),
    ("2024-06-15 Q3", "%Y-%m-%d Q%q"),
    ("2024-06-15 Q5", "%Y-%m-%d Q%q"),
    ("2024 Q2", "%Y Q%q"),
    ("2024-06-15 Sat", "%Y-%m-%d %a"),
    ("2024-06-15 Sun", "%Y-%m-%d %a"),
    ("2024-06-15 167", "%Y-%m-%d %j"),
    ("2024-06-15 168", "%Y-%m-%d %j"),
    ("2024-06-15 23", "%Y-%m-%d %U"),
    ("2024-06-15 22", "%Y-%m-%d %U"),
    ("2024-06-15 24", "%Y-%m-%d %V"),
    ("2024-06-15 2024", "%Y-%m-%d %G"),
    ("2024-12-30 2025", "%Y-%m-%d %G"),
    ("2024-12-30 2024", "%Y-%m-%d %G"),
    // Composite date specifiers.
    ("06/15/24", "%D"),
    ("06/15/24", "%x"),
    ("06/15/2024", "%D"),
    ("2024-06-15", "%F"),
    ("15-Jun-2024", "%v"),
    (" 5-Jun-2024", "%v"),
    ("5-Jun-2024", "%v"),
    ("Sat Jun 15 10:30:00 2024", "%c"),
    ("Sat Jun  5 10:30:00 2024", "%c"),
    ("Wed Jun  5 10:30:00 2024", "%c"),
    ("sat jun 15 10:30:00 2024", "%c"),
    // Missing or conflicting fields.
    ("2024-06", "%Y-%m"),
    ("06-15", "%m-%d"),
    ("2024", "%Y"),
    ("15", "%d"),
    ("2024-06-15 2024-06-16", "%Y-%m-%d %Y-%m-%d"),
    ("2024-06-15 2024-06-15", "%Y-%m-%d %Y-%m-%d"),
    ("2024-06-15 07", "%Y-%m-%d %m"),
    ("", "%Y-%m-%d"),
    ("", ""),
    ("x", ""),
    ("2024-06-15", ""),
    ("2024-06-15x", "%Y-%m-%d"),
    ("2024-06-15 ", "%Y-%m-%d"),
    (" 2024-06-15", "%Y-%m-%d"),
    ("2024-06-15", "%Y-%m-%d "),
    ("2024-06-15", " %Y-%m-%d"),
    ("2024-06", "%Y-%m-%d"),
    ("2024-06-", "%Y-%m-%d"),
    ("2024/06/15", "%Y-%m-%d"),
    ("abcd-06-15", "%Y-%m-%d"),
    ("2024-06-15", "%Y-%m-%d %"),
    ("2024-06-15", "%Y-%m-%d %i"),
    ("2024-06-15", "%Y-%m-%d %#m"),
    ("2024-06-15", "%Y-%m-%d %03d"),
    // Literals and whitespace.
    ("2024  -  06  -  15", "%Y - %m - %d"),
    ("2024-06-15", "%Y - %m - %d"),
    ("2024\t06\n15", "%Y %m %d"),
    ("2024 06 15", "%Y%t%m%n%d"),
    ("20240615", "%Y%t%m%n%d"),
    ("2024\u{a0}06\u{3000}15", "%Y %m %d"),
    ("100% 2024-06-15", "100%% %Y-%m-%d"),
    ("date: 2024-06-15", "date: %Y-%m-%d"),
    ("Date: 2024-06-15", "date: %Y-%m-%d"),
    ("日付 2024-06-15 é", "日付 %Y-%m-%d é"),
    ("日付 2024-06-15", "日付 %Y-%m-%d é"),
    ("日", "é%Y"),
];

const TIME_PARSE: &[(&str, &str)] = &[
    ("2024-06-15 10:30:45", "%Y-%m-%d %H:%M:%S"),
    ("2024-06-15 10:30", "%Y-%m-%d %H:%M"),
    ("2024-06-15 10", "%Y-%m-%d %H"),
    ("2024-06-15 :30", "%Y-%m-%d :%M"),
    ("2024-06-15 10::45", "%Y-%m-%d %H::%S"),
    ("2024-06-15 1:2:3", "%Y-%m-%d %H:%M:%S"),
    ("2024-06-15  1: 2: 3", "%Y-%m-%d %k:%M:%S"),
    ("2024-06-15 24:00:00", "%Y-%m-%d %H:%M:%S"),
    ("2024-06-15 23:60:00", "%Y-%m-%d %H:%M:%S"),
    ("2024-06-15 23:59:60", "%Y-%m-%d %H:%M:%S"),
    ("2024-06-15 10:30:60", "%Y-%m-%d %H:%M:%S"),
    ("2024-06-15 23:59:61", "%Y-%m-%d %H:%M:%S"),
    ("2024-06-15 10:30:45", "%F %T"),
    ("2024-06-15 10:30", "%F %R"),
    ("2024-06-15 10:30:45", "%F %X"),
    ("2024-06-15 10:30:45 AM", "%F %r"),
    ("2024-06-15 10:30:45 pm", "%F %r"),
    ("2024-06-15 12:00:00 AM", "%F %r"),
    ("2024-06-15 12:00:00 PM", "%F %r"),
    ("2024-06-15 13:00:00 PM", "%F %r"),
    ("2024-06-15 00:00:00 AM", "%F %r"),
    ("2024-06-15 10:30 AM", "%F %I:%M %p"),
    ("2024-06-15 10:30 pm", "%F %I:%M %P"),
    ("2024-06-15 10:30 PM", "%F %I:%M %P"),
    ("2024-06-15 10:30PM", "%F %I:%M%p"),
    ("2024-06-15 10:30 P", "%F %I:%M %p"),
    ("2024-06-15 10:30 XM", "%F %I:%M %p"),
    ("2024-06-15 10:30", "%F %I:%M"),
    ("2024-06-15 10:30 PM", "%F %H:%M %p"),
    ("2024-06-15 22:30 PM", "%F %H:%M %p"),
    ("2024-06-15 22:30 AM", "%F %H:%M %p"),
    ("2024-06-15 22 10 PM", "%F %H %I %p"),
    ("2024-06-15 22 11 PM", "%F %H %I %p"),
    ("2024-06-15  9:30 AM", "%F %l:%M %p"),
    ("2024-06-15 PM", "%F %p"),
    // Fractions of a second.
    ("2024-06-15 10:30:45.250", "%F %T%.f"),
    ("2024-06-15 10:30:45", "%F %T%.f"),
    ("2024-06-15 10:30:45.", "%F %T%.f"),
    ("2024-06-15 10:30:45.1", "%F %T%.f"),
    ("2024-06-15 10:30:45.123456789", "%F %T%.f"),
    ("2024-06-15 10:30:45.1234567891234", "%F %T%.f"),
    ("2024-06-15 10:30:45,250", "%F %T%.f"),
    ("2024-06-15 10:30:45.250", "%F %T%.3f"),
    ("2024-06-15 10:30:45.25", "%F %T%.3f"),
    ("2024-06-15 10:30:45.2500", "%F %T%.3f"),
    ("2024-06-15 10:30:45", "%F %T%.3f"),
    ("2024-06-15 10:30:45.250000", "%F %T%.6f"),
    ("2024-06-15 10:30:45.250000000", "%F %T%.9f"),
    ("2024-06-15 10:30:45.250", "%F %T.%3f"),
    ("2024-06-15 10:30:45.25", "%F %T.%3f"),
    ("2024-06-15 10:30:45.250000", "%F %T.%6f"),
    ("2024-06-15 10:30:45.250000000", "%F %T.%9f"),
    ("2024-06-15 10:30:45.250", "%F %T.%f"),
    ("2024-06-15 10:30:45.000000250", "%F %T.%f"),
    ("2024-06-15 10:30:45.999999999", "%F %T.%f"),
    ("2024-06-15 10:30:45.1234567890", "%F %T.%f"),
    ("2024-06-15 10:30.250", "%F %H:%M%.f"),
    ("2024-06-15 10:30:45 250 250", "%F %T %f %3f"),
    ("2024-06-15 10:30:45 250 251", "%F %T %f %3f"),
    // Unix timestamps.
    ("1718447400", "%s"),
    ("0", "%s"),
    ("-1", "%s"),
    ("+1", "%s"),
    ("-62135596800", "%s"),
    ("253402300799", "%s"),
    ("253402300800", "%s"),
    ("-377705116800", "%s"),
    ("-377705116801", "%s"),
    ("8210266876799", "%s"),
    ("8210266876800", "%s"),
    ("9223372036854775807", "%s"),
    ("9223372036854775808", "%s"),
    ("1718447400.5", "%s%.f"),
    ("1718447400 +0200", "%s %z"),
    ("1718447400 2024", "%s %Y"),
    ("1718447400 2025", "%s %Y"),
    ("1718447400 2024-06-15 10:30:00", "%s %F %T"),
    ("1718447400 2024-06-15 10:30:01", "%s %F %T"),
    ("1718447400 2024-06-15 12:30:00 +0200", "%s %F %T %z"),
    ("1718447400 10", "%s %H"),
    ("1718447400 11", "%s %H"),
    ("1718447400 Sat", "%s %a"),
    ("1718447400 Sun", "%s %a"),
    ("1718447400 60", "%s %S"),
    ("1718447399 60", "%s %S"),
    ("1.5", "%s"),
    ("abc", "%s"),
    ("", "%s"),
];

const OFFSET_PARSE: &[(&str, &str)] = &[
    ("2024-06-15 10:30:00 +0200", "%F %T %z"),
    ("2024-06-15 10:30:00 +02:00", "%F %T %z"),
    ("2024-06-15 10:30:00 +02 00", "%F %T %z"),
    ("2024-06-15 10:30:00 +02::00", "%F %T %z"),
    ("2024-06-15 10:30:00 +02", "%F %T %z"),
    ("2024-06-15 10:30:00 +2", "%F %T %z"),
    ("2024-06-15 10:30:00 +020", "%F %T %z"),
    ("2024-06-15 10:30:00 +02000", "%F %T %z"),
    ("2024-06-15 10:30:00 0200", "%F %T %z"),
    ("2024-06-15 10:30:00 Z", "%F %T %z"),
    ("2024-06-15 10:30:00 -0000", "%F %T %z"),
    ("2024-06-15 10:30:00 -0330", "%F %T %z"),
    ("2024-06-15 10:30:00 \u{2212}0330", "%F %T %z"),
    ("2024-06-15 10:30:00 +2359", "%F %T %z"),
    ("2024-06-15 10:30:00 +2400", "%F %T %z"),
    ("2024-06-15 10:30:00 +9959", "%F %T %z"),
    ("2024-06-15 10:30:00 +0260", "%F %T %z"),
    ("2024-06-15 10:30:00 +02:3", "%F %T %z"),
    ("2024-06-15 10:30:00+0200", "%F %T%z"),
    ("2024-06-15 10:30:00   +0200", "%F %T%z"),
    ("2024-06-15 10:30:00 +0200", "%F %T %:z"),
    ("2024-06-15 10:30:00 +02:00", "%F %T %:z"),
    ("2024-06-15 10:30:00 +02:00:00", "%F %T %::z"),
    ("2024-06-15 10:30:00 +02:00", "%F %T %::z"),
    ("2024-06-15 10:30:00 +02", "%F %T %:::z"),
    ("2024-06-15 10:30:00 +02:00", "%F %T %:::z"),
    ("2024-06-15 10:30:00 +02", "%F %T %#z"),
    ("2024-06-15 10:30:00 +0200", "%F %T %#z"),
    ("2024-06-15 10:30:00 +02:30", "%F %T %#z"),
    ("2024-06-15 10:30:00 Z", "%F %T %#z"),
    ("2024-06-15 10:30:00 z", "%F %T %#z"),
    ("2024-06-15 10:30:00 +02 UTC", "%F %T %#z %Z"),
    ("2024-06-15 10:30:00 +0200 +0200", "%F %T %z %z"),
    ("2024-06-15 10:30:00 +0200 +0300", "%F %T %z %z"),
    ("2024-06-15 +0200", "%F %z"),
    ("10:30 +0200", "%H:%M %z"),
    ("2024-06-15 10:30:00 UTC", "%F %T %Z"),
    ("2024-06-15 10:30:00 gmt", "%F %T %Z"),
    ("2024-06-15 10:30:00 CEST", "%F %T %Z"),
    ("2024-06-15 10:30:00 Europe/Berlin", "%F %T %Z"),
    ("2024-06-15 10:30:00 ", "%F %T %Z"),
    ("2024-06-15 10:30:00", "%F %T %Z"),
    ("2024-06-15 10:30:00 UTC trailing", "%F %T %Z"),
    ("2024-06-15 10:30:00 UTC UTC", "%F %T %Z %Z"),
    ("2024-06-15 10:30:00 UTC CEST", "%F %T %Z %Z"),
    ("CEST 2024-06-15 10:30:00", "%Z %F %T"),
    ("UTC 2024-06-15 10:30:00", "%Z %F %T"),
    ("2024-06-15 10:30:00 CEST +0200", "%F %T %Z %z"),
    ("2024-06-15 10:30:00 +0200 CEST", "%F %T %z %Z"),
    // `%+`.
    ("2024-06-15T10:30:00+02:00", "%+"),
    ("2024-06-15T10:30:00Z", "%+"),
    ("2024-06-15t10:30:00z", "%+"),
    ("2024-06-15 10:30:00 UTC", "%+"),
    ("2024-06-15 10:30:00 utc", "%+"),
    ("2024-06-15T10:30:00.250+0200", "%+"),
    ("2024-06-15T10:30:00 +02", "%+"),
    ("2024-06-15T10:30:00", "%+"),
    ("2024-06-15T10:30+02:00", "%+"),
    ("2024-6-5T1:2:3Z", "%+"),
    ("2024 - 06 - 15T10 : 30 : 00 Z", "%+"),
    ("+12345-06-15T10:30:00Z", "%+"),
    ("2024-06-15X10:30:00Z", "%+"),
    ("2024-06-15", "%+"),
    ("2024-06-15T10:30:00Z tail", "%+ tail"),
    ("2024-06-15T23:59:60Z", "%+"),
    ("x 2024-06-15T10:30:00-05:00 y", "x %+ y"),
];

/// Inputs with no `input_format`: each auto-detected shape and its near misses.
const AUTO: &[&str] = &[
    "2024-06-15T10:30:00Z",
    "2024-06-15T10:30:00z",
    "2024-06-15t10:30:00Z",
    "2024-06-15 10:30:00Z",
    "2024-06-15T10:30:00+02:00",
    "2024-06-15T10:30:00-00:00",
    "2024-06-15T10:30:00+00:00",
    "2024-06-15T10:30:00+23:59",
    "2024-06-15T10:30:00-23:59",
    "2024-06-15T10:30:00+24:00",
    "2024-06-15T10:30:00+02:60",
    "2024-06-15T10:30:00+0200",
    "2024-06-15T10:30:00+02",
    "2024-06-15T10:30:00 +02:00",
    "2024-06-15T10:30:00\u{2212}02:00",
    "2024-06-15T10:30:00.1Z",
    "2024-06-15T10:30:00.123456789Z",
    "2024-06-15T10:30:00.1234567891234Z",
    "2024-06-15T10:30:00.Z",
    "2024-06-15T10:30:00,5Z",
    "2024-06-15T23:59:60Z",
    "2024-06-15T10:30:60.5+02:00",
    "2024-06-15T10:30:61Z",
    "2024-06-15T24:00:00Z",
    "2024-06-15T10:60:00Z",
    "2024-02-30T10:30:00Z",
    "2023-02-29T10:30:00Z",
    "2024-02-29T10:30:00Z",
    "2024-06-15T10:30Z",
    "2024-06-15T10:30:00",
    "2024-06-15T10:30:00.5",
    "2024-06-15 10:30:00",
    "2024-06-15 10:30:00.5",
    "2024-06-15  10:30:00",
    "2024-06-15 10:30",
    "2024-6-5 1:2:3",
    "2024-6-5T1:2:3",
    "2024-06-15",
    "2024-6-5",
    " 2024-06-15",
    "2024-06-15 ",
    " 2024-06-15T10:30:00Z",
    "2024-06-15T10:30:00Z ",
    "20240615T103000Z",
    "2024-06-15T10:30:00Z[UTC]",
    "2024-06-15T10:30:00+02:00[Europe/Berlin]",
    "0000-01-01T00:00:00Z",
    "9999-12-31T23:59:59.999999999Z",
    "9999-12-31T23:59:59-23:59",
    "0000-01-01T00:00:00+23:59",
    "10000-01-01T00:00:00Z",
    "+2024-06-15T10:30:00Z",
    "-2024-06-15T10:30:00Z",
    "+12345-06-15",
    "-0044-03-15",
    "-0044-03-15 12:00:00",
    "12345-06-15",
    "1969-12-31T23:59:59Z",
    "1900-01-01",
    // RFC 2822.
    "Sat, 15 Jun 2024 10:30:00 +0000",
    "Sat, 15 Jun 2024 10:30:00 +0200",
    "Sat, 15 Jun 2024 10:30:00 -0000",
    "15 Jun 2024 10:30:00 +0200",
    "Sat, 15 Jun 2024 10:30 +0200",
    "sat, 15 jun 2024 10:30:00 +0200",
    "Sun, 15 Jun 2024 10:30:00 +0200",
    "Sat 15 Jun 2024 10:30:00 +0200",
    "Saturday, 15 Jun 2024 10:30:00 +0200",
    "Sat, 15 June 2024 10:30:00 +0200",
    "Sat,15 Jun 2024 10:30:00 +0200",
    "  Sat,   15   Jun   2024   10 : 30 : 00   +0200",
    "Sat, 5 Jun 2024 10:30:00 +0200",
    "Wed, 5 Jun 2024 10:30:00 +0200",
    "Wed, 05 Jun 2024 10:30:00 +0200",
    "Wed, 005 Jun 2024 10:30:00 +0200",
    "Sat, 15 Jun 24 10:30:00 +0200",
    "15 Jun 49 10:30:00 +0000",
    "15 Jun 50 10:30:00 +0000",
    "15 Jun 124 10:30:00 +0000",
    "15 Jun 0654 10:30:00 +0000",
    "15 Jun 12345 10:30:00 +0000",
    "15 Jun 2 10:30:00 +0000",
    "Sat, 15 Jun 2024 10:30:00 GMT",
    "Sat, 15 Jun 2024 10:30:00 gmt",
    "Sat, 15 Jun 2024 10:30:00 UT",
    "Sat, 15 Jun 2024 10:30:00 UTC",
    "Sat, 15 Jun 2024 10:30:00 Z",
    "Sat, 15 Jun 2024 10:30:00 EST",
    "Sat, 15 Jun 2024 10:30:00 EDT",
    "Sat, 15 Jun 2024 10:30:00 CST",
    "Sat, 15 Jun 2024 10:30:00 CDT",
    "Sat, 15 Jun 2024 10:30:00 MST",
    "Sat, 15 Jun 2024 10:30:00 MDT",
    "Sat, 15 Jun 2024 10:30:00 PST",
    "Sat, 15 Jun 2024 10:30:00 PDT",
    "Sat, 15 Jun 2024 10:30:00 A",
    "Sat, 15 Jun 2024 10:30:00 J",
    "Sat, 15 Jun 2024 10:30:00 CEST",
    "Sat, 15 Jun 2024 10:30:00 +02:00",
    "Sat, 15 Jun 2024 10:30:00 +02",
    "Sat, 15 Jun 2024 10:30:00 0200",
    "Sat, 15 Jun 2024 10:30:00 +2400",
    "Sat, 15 Jun 2024 10:30:00 +0260",
    "Sat, 15 Jun 2024 10:30:00",
    "Sat, 15 Jun 2024 10:30:00 +0200 (CEST)",
    "Sat, 15 Jun 2024 10:30:00 +0200 (a (nested) \\) comment) (second)",
    "Sat, 15 Jun 2024 10:30:00 +0200 (open",
    "Sat, 15 Jun 2024 10:30:00 +0200 tail",
    "Sat, 15 Jun 2024 10:30:00 +0200 ",
    "Sat, 15 Jun 2024 10:30:60 +0000",
    "Sat, 15 Jun 2024 24:30:00 +0000",
    "Sat, 15 Jun 2024 1:30:00 +0000",
    "Fri, 31 Dec 9999 23:59:59 +0000",
    "Thu, 01 Jan 1970 00:00:00 +0000",
    "Mon, 30 Feb 2024 10:30:00 +0000",
    // Not dates.
    "",
    " ",
    "not-a-date",
    "now ",
    "today",
    "1718447400",
    "2024",
    "2024-06",
    "15/06/2024",
    "June 15, 2024",
    "2024-06-15T",
    "日付",
];

const ZONE_PARAMETER: &[&str] = &[
    "UTC",
    "utc",
    "Utc",
    "Z",
    "z",
    "GMT",
    "",
    " ",
    " UTC ",
    "+00:00",
    "-00:00",
    "+02:00",
    "-05:30",
    " +02:00 ",
    "+2:00",
    "+5:30",
    "+02:0",
    "+02:000",
    "+002:00",
    "+0200",
    "+02",
    "02:00",
    "+02:60",
    "+02:-30",
    "+-2:00",
    "-+2:00",
    "+02:+30",
    "+23:59",
    "-23:59",
    "+24:00",
    "-24:00",
    "+23:60",
    "+99:99",
    "+02:00:00",
    "+:",
    "+ 2:00",
    "\u{2212}02:00",
    "Europe/Berlin",
    "CEST",
];

fn parameter_cases(cases: &mut Vec<Value>) {
    let out = "%Y-%m-%dT%H:%M:%S%.f%:z %Z";
    for zone in ZONE_PARAMETER {
        cases.push(format_case(INSTANTS[0], None, Some(zone), out));
    }
    // A conversion that leaves the calendar range, in both directions.
    cases.push(format_case(
        "9999-12-31T23:59:59Z",
        None,
        Some("+14:00"),
        out,
    ));
    cases.push(format_case(
        "0000-01-01T00:00:00Z",
        None,
        Some("-14:00"),
        out,
    ));
    let wide = Some("%Y-%m-%d %H:%M:%S");
    cases.push(format_case(
        "+262142-12-31 23:59:59",
        wide,
        Some("+14:00"),
        out,
    ));
    cases.push(format_case(
        "-262143-01-01 00:00:00",
        wide,
        Some("-14:00"),
        out,
    ));

    // Parameter combinations and the values `source_key` may hold.
    cases.push(json!({ "config": {} }));
    cases.push(json!({ "config": { "input": "2024-06-15", "source_key": "when" } }));
    cases.push(json!({ "config": { "input": 5 } }));
    cases.push(json!({ "config": { "source_key": "missing" } }));
    cases.push(json!({ "config": { "input": "2024-06-15T10:30:00Z" } }));
    cases.push(json!({ "config": { "input": "${ctx.when}" }, "ctx": { "when": "2024-06-15" } }));
    cases.push(
        json!({ "config": { "input": "${ctx.when}T10:30:00Z" }, "ctx": { "when": "2024-06-15" } }),
    );
    for held in [
        json!("2024-06-15T10:30:00+02:00"),
        json!("2024-06-15"),
        json!(1_718_447_400),
        json!(1_718_447_400.5),
        json!(true),
        json!(null),
        json!(["2024-06-15"]),
        json!({ "date": "2024-06-15" }),
    ] {
        cases.push(json!({ "config": { "source_key": "when" }, "ctx": { "when": held } }));
    }
    cases.push(json!({
        "config": { "source_key": "when", "input_format": "%s" },
        "ctx": { "when": 1_718_447_400 }
    }));
    cases.push(json!({
        "config": { "input": "2024-06-15", "timezone": 5, "output_format": 7, "output_key": 9 }
    }));
    cases.push(json!({
        "config": { "input": "2024-06-15", "input_format": 5 }
    }));
}

fn cases() -> Vec<Value> {
    let mut cases = Vec::new();
    output_cases(&mut cases);
    read_back_cases(&mut cases);
    for group in [DATE_PARSE, TIME_PARSE, OFFSET_PARSE] {
        cases.extend(
            group
                .iter()
                .map(|(input, format)| parse_case(input, format)),
        );
    }
    cases.extend(AUTO.iter().map(|input| auto_case(input)));
    parameter_cases(&mut cases);
    cases
}

async fn execute(config: &Value, ctx: &Context) -> Value {
    let registry = NodeRegistry::with_builtins();
    let node = registry
        .get("date_format")
        .expect("date_format is built in");
    match node.execute(config, ctx).await {
        Ok(output) => {
            let key = config["output_key"].as_str().unwrap_or("formatted_date");
            json!({ "ok": [output[key], output[&format!("{key}_unix")]] })
        }
        Err(error) => json!({ "err": error.to_string() }),
    }
}

/// What the node returns for one case.
async fn observe(case: &Value) -> Value {
    let ctx: Context = case
        .get("ctx")
        .and_then(Value::as_object)
        .map(|held| held.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
        .unwrap_or_default();
    let Some(read_back) = case.get("read_back") else {
        return execute(&case["config"], &ctx).await;
    };
    // Format with a specifier after a full date and time, then read the
    // text back with the same format.
    let format = format!("%Y-%m-%d %H:%M:%S {}", read_back["spec"].as_str().unwrap());
    let written = execute(
        &json!({ "input": read_back["input"], "output_format": format }),
        &ctx,
    )
    .await;
    let Some(text) = written["ok"][0].as_str() else {
        return json!({ "written": written });
    };
    let read = execute(
        &json!({
            "input": text,
            "input_format": format,
            "output_format": "%Y-%m-%dT%H:%M:%S%.f%:z"
        }),
        &ctx,
    )
    .await;
    json!({ "written": written, "read": read })
}

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/nodes/fixtures/date_format_golden.jsonl")
}

/// Run a case on its own thread so a panic is recorded, not propagated.
fn observe_isolated(case: Value) -> Value {
    std::thread::spawn(move || {
        tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .block_on(observe(&case))
    })
    .join()
    .unwrap_or_else(|_| json!({ "panic": true }))
}

#[test]
#[ignore = "rewrites the golden fixture; run only to record an intended change"]
fn record() {
    let mut text = String::new();
    for case in cases() {
        let observed = observe_isolated(case.clone());
        text.push_str(&serde_json::to_string(&json!([case, observed])).unwrap());
        text.push('\n');
    }
    std::fs::create_dir_all(fixture_path().parent().unwrap()).unwrap();
    std::fs::write(fixture_path(), text).unwrap();
}

/// Inputs that name a year outside -9999 to 9999.
const BEYOND_THE_RANGE: [&str; 14] = [
    "-10000-12-31 23:59:59",
    "+10000-01-01 00:00:00",
    "+12345-06-07 01:02:03",
    "+262142-12-31 23:59:59",
    "-262143-01-01 00:00:00",
    "+12345-06-15",
    "+10000-01-01",
    "-10000-01-01",
    "+262142-12-31",
    "-262143-01-01",
    "253402300800",
    "8210266876799",
    "+12345-06-15T10:30:00Z",
    "15 Jun 12345 10:30:00 +0000",
];

/// Inputs inside the range that a `timezone` moves out of it.
const LEAVES_THE_RANGE: [(&str, &str); 4] = [
    ("9999-12-31T23:59:59Z", "+05:30"),
    ("9999-12-31T23:59:59Z", "+14:00"),
    ("-9999-01-01 00:00:00", "-08:00"),
    ("-9999-01-01 00:00:00", "-00:01"),
];

/// The accepted difference from the recording (IF-231): `chrono` held the
/// years -262143 to 262142 and `jiff` holds -9999 to 9999, so a date
/// outside them is an error now. Returns the error such a case must give.
fn beyond_the_range(case: &Value) -> Option<String> {
    let config = &case["config"];
    let input = config["input"].as_str()?;
    let format = config["input_format"].as_str();
    if BEYOND_THE_RANGE.contains(&input) {
        return Some(match format {
            Some(format) => {
                format!("Failed to parse '{input}' with format '{format}': input is out of range")
            }
            None => format!(
                "Could not parse '{input}' as a date. Supported formats: RFC3339, RFC2822, \
                 YYYY-MM-DD HH:MM:SS, YYYY-MM-DDTHH:MM:SS, YYYY-MM-DD"
            ),
        });
    }
    let zone = config["timezone"].as_str()?;
    LEAVES_THE_RANGE.contains(&(input, zone)).then(|| {
        format!("date_format: the date at timezone '{zone}' is outside the years -9999 to 9999")
    })
}

/// The corrected defect in the recording (IF-234): `%C` in a year before
/// 0000 was written as `I5` (years -0100 to -0001) or `?6` (year -9999),
/// neither of which is a number. The century is now `-1` or `-100`. No other
/// output in the fixture contains those two characters together.
fn with_numeric_century(recorded: &Value) -> Value {
    let Some(text) = recorded["ok"][0].as_str() else {
        return recorded.clone();
    };
    let corrected = text.replace("I5", "-1").replace("?6", "-100");
    json!({ "ok": [corrected, recorded["ok"][1]] })
}

/// IF-257: these ten malformed output offsets succeeded in the old recording.
fn invalid_output_timezone(case: &Value) -> Option<String> {
    let zone = case["config"]["timezone"].as_str()?;
    matches!(
        zone,
        "+2:00"
            | "+5:30"
            | "+02:0"
            | "+02:000"
            | "+002:00"
            | "+02:60"
            | "+02:-30"
            | "+-2:00"
            | "-+2:00"
            | "+02:+30"
    )
    .then(|| format!("Invalid timezone '{zone}'. Use 'UTC', '+HH:MM', or '-HH:MM'"))
}

/// IF-238: the date-only fallback used to hide these time resolution errors.
fn date_time_failure(case: &Value, recorded: &Value) -> Option<Value> {
    let impossible = "no possible date and time matching input";
    let error = |input: &str, format: &str, reason: &str| json!({"err": format!("Failed to parse '{input}' with format '{format}': {reason}")});
    if let Some(back) = case.get("read_back") {
        // These non-UTC wall times were written without an offset, alongside %s.
        if back["spec"] == "%s"
            && matches!(
                back["input"].as_str()?,
                "2021-01-03T12:00:00-08:00" | "2024-12-30T23:59:59.000001+05:30"
            )
        {
            let mut expected = recorded.clone();
            expected["read"] = error(
                recorded["written"]["ok"][0].as_str()?,
                "%Y-%m-%d %H:%M:%S %s",
                impossible,
            );
            return Some(expected);
        }
        return None;
    }
    let input = case["config"]["input"].as_str()?;
    let format = case["config"]["input_format"].as_str()?;
    let reason = match (format, input) {
        ("%Y-%m-%d %H", "2024-06-15 10")
        | ("%Y-%m-%d :%M", "2024-06-15 :30")
        | ("%Y-%m-%d %H::%S", "2024-06-15 10::45")
        | ("%F %I:%M", "2024-06-15 10:30")
        | ("%F %H %I %p", "2024-06-15 22 10 PM")
        | ("%F %p", "2024-06-15 PM")
        | ("%F %H:%M%.f", "2024-06-15 10:30.250") => "input is not enough for unique date and time",
        ("%s", "8210266876800" | "9223372036854775807") => "input is out of range",
        ("%s %Y", "1718447400 2025")
        | ("%s %F %T", "1718447400 2024-06-15 10:30:01")
        | ("%s %H", "1718447400 11")
        | ("%s %a", "1718447400 Sun") => impossible,
        _ => return None,
    };
    Some(error(input, format, reason))
}

#[test]
fn date_format_matches_the_recorded_behaviour() {
    let recorded = std::fs::read_to_string(fixture_path()).expect("golden fixture");
    let recorded: Vec<Value> = recorded
        .lines()
        .map(|line| serde_json::from_str(line).expect("fixture line is JSON"))
        .collect();
    let cases = cases();
    assert_eq!(
        recorded.len(),
        cases.len(),
        "fixture and case list differ in length"
    );

    let mut differences = Vec::new();
    let mut beyond = 0;
    for (case, line) in cases.into_iter().zip(&recorded) {
        assert_eq!(&line[0], &case, "fixture is out of step with the case list");
        let observed = observe_isolated(case.clone());
        let expected = match beyond_the_range(&case) {
            Some(error) if line[1].get("ok").is_some() => {
                beyond += 1;
                json!({ "err": error })
            }
            _ if let Some(error) = invalid_output_timezone(&case) => json!({ "err": error }),
            _ if let Some(corrected) = date_time_failure(&case, &line[1]) => corrected,
            _ => with_numeric_century(&line[1]),
        };
        if observed != expected {
            differences.push(format!(
                "case     {case}\nexpected {expected}\nobserved {observed}"
            ));
        }
    }
    assert!(
        differences.is_empty(),
        "{} of {} cases differ:\n\n{}",
        differences.len(),
        recorded.len(),
        differences.join("\n\n")
    );
    // Every listed case was recorded as a value; none is silently skipped.
    assert_eq!(beyond, 81, "cases beyond the year range");
}
