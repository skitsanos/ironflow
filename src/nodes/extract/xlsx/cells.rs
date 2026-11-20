//! Converting one spreadsheet cell into a JSON value.

use calamine::{Data, ExcelDateTime, ExcelDateTimeType};
use jiff::SignedDuration;
use jiff::civil::DateTime;
use serde_json::Value;

const UNIX_EPOCH: DateTime = DateTime::constant(1970, 1, 1, 0, 0, 0, 0);
/// Milliseconds from day zero of serial dates, 1899-12-30, to 1970-01-01.
/// Excel counts 1900-02-29, which never existed, so serials below 60 are one
/// day further from day zero than they claim.
const DAY_ZERO_TO_UNIX_EPOCH: i64 = 25_569 * 86_400_000;
/// Days between the 1900 and the 1904 date systems.
const DAYS_TO_1904: f64 = 1462.0;
const DAY_MILLISECONDS: f64 = 86_400_000.0;
/// 2^63: the first millisecond count an `i64` cannot hold.
const MILLISECONDS_LIMIT: f64 = 9_223_372_036_854_775_808.0;

/// Convert a cell to the value a flow sees.
///
/// Blanks and Excel errors both become `null`: a consumer treats "no usable
/// value" uniformly rather than learning Excel's error taxonomy. The cost is
/// that a flow auditing a workbook cannot tell a broken formula from an empty
/// cell.
pub(super) fn cell_value(cell: &Data) -> Value {
    match cell {
        Data::Int(value) => Value::from(*value),
        Data::Float(value) => number_value(*value),
        Data::String(value) => Value::String(value.clone()),
        Data::Bool(value) => Value::Bool(*value),
        // `.xlsx` has no date type — a date is a float plus a number-format
        // code. Emitting the serial would make every flow re-derive the format
        // lookup and the 1900-epoch leap-year quirk Excel preserves.
        // An elapsed-time format (`[h]:mm:ss`) is a length of time, not a
        // point on the calendar; reading it as a date drops whole days.
        Data::DateTime(value) if value.is_duration() => {
            milliseconds(value.as_f64()).map_or(Value::Null, |ms| Value::String(iso_duration(ms)))
        }
        Data::DateTime(value) => date_time(value).map_or(Value::Null, Value::String),
        Data::DateTimeIso(value) | Data::DurationIso(value) => Value::String(value.clone()),
        Data::Error(_) | Data::Empty => Value::Null,
    }
}

/// A number of days as whole milliseconds, or `None` when it is not a
/// number or does not fit 64 bits. Cell values are untrusted: `1e300`,
/// `inf` and `NaN` are all valid cell text.
fn milliseconds(days: f64) -> Option<i64> {
    let ms = (days * DAY_MILLISECONDS).round();
    // Inside the limit the cast is exact.
    (ms.abs() < MILLISECONDS_LIMIT).then_some(ms as i64)
}

/// A date serial as `YYYY-MM-DDTHH:MM:SS`, or `None` when it names no date
/// in the years -9999 to 9999.
fn date_time(value: &ExcelDateTime) -> Option<String> {
    let serial = value.as_f64();
    // The date system is not readable from the value, but two values compare
    // equal only when they share one.
    let in_1904 = *value == ExcelDateTime::new(serial, ExcelDateTimeType::DateTime, true);
    let days = if in_1904 {
        serial + DAYS_TO_1904
    } else {
        serial
    };
    let days = if days >= 60.0 { days } else { days + 1.0 };
    // Counted from 1970 because `jiff` refuses a duration longer than the
    // distance from 1970 to the end of its range, wherever the sum lands.
    let since_1970 = milliseconds(days)?.checked_sub(DAY_ZERO_TO_UNIX_EPOCH)?;
    let at = UNIX_EPOCH
        .checked_add(SignedDuration::from_millis(since_1970))
        .ok()?;
    let year = at.year();
    let sign = if year < 0 { "-" } else { "" };
    Some(format!(
        "{sign}{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
        year.unsigned_abs(),
        at.month(),
        at.day(),
        at.hour(),
        at.minute(),
        at.second()
    ))
}

/// Render milliseconds as an ISO-8601 duration with hours as the largest unit
/// (`PT37H30M`), so the value is exact and needs no calendar assumptions.
/// Zero is `PT0S`, a negative span is prefixed with `-`, and fractional
/// seconds keep up to three digits (calamine rounds to milliseconds).
fn iso_duration(total_ms: i64) -> String {
    let sign = if total_ms < 0 { "-" } else { "" };
    let ms = total_ms.unsigned_abs();
    let (hours, minutes) = (ms / 3_600_000, ms / 60_000 % 60);
    let (seconds, millis) = (ms / 1000 % 60, ms % 1000);
    if ms == 0 {
        return "PT0S".to_string();
    }
    let mut text = format!("{sign}PT");
    if hours > 0 {
        text.push_str(&format!("{hours}H"));
    }
    if minutes > 0 {
        text.push_str(&format!("{minutes}M"));
    }
    if seconds > 0 || millis > 0 {
        text.push_str(&seconds.to_string());
        if millis > 0 {
            let fraction = format!("{millis:03}");
            text.push('.');
            text.push_str(fraction.trim_end_matches('0'));
        }
        text.push('S');
    }
    text
}

/// Emit a whole number as an integer when it round-trips exactly.
///
/// Every `.xlsx` number is a double, so without this a quantity column reaches
/// Lua as `3.0` rather than `3` — Lua 5.4 distinguishes the two, and the noise
/// reaches every downstream comparison and interpolation.
fn number_value(value: f64) -> Value {
    // i64::MAX as f64 rounds UP to 2^63, which is one past the real i64::MAX,
    // so a <= bound would admit a value that doesn't round-trip. Use a strict
    // bound with the literal 2^63 to exclude it.
    if value.fract() == 0.0 && value >= i64::MIN as f64 && value < 9223372036854775808.0_f64 {
        let truncated = value as i64;
        if truncated as f64 == value {
            return Value::from(truncated);
        }
    }
    Value::from(value)
}

#[cfg(test)]
mod tests {
    use super::cell_value;
    use calamine::{CellErrorType, Data, ExcelDateTime, ExcelDateTimeType};
    use serde_json::json;

    #[test]
    fn whole_numbers_become_integers_and_fractions_stay_floats() {
        // xlsx stores every number as a double, so a quantity column would
        // otherwise reach Lua as 3.0 rather than 3.
        assert_eq!(cell_value(&Data::Float(3.0)), json!(3));
        assert_eq!(cell_value(&Data::Float(-7.0)), json!(-7));
        assert_eq!(cell_value(&Data::Float(2.5)), json!(2.5));
        assert_eq!(cell_value(&Data::Int(42)), json!(42));
    }

    #[test]
    fn numbers_too_large_for_exact_integers_stay_floats() {
        let huge = 9.3e18_f64;
        assert!(cell_value(&Data::Float(huge)).is_f64());
    }

    #[test]
    fn text_and_booleans_pass_through() {
        assert_eq!(cell_value(&Data::String("Acme".into())), json!("Acme"));
        assert_eq!(cell_value(&Data::Bool(true)), json!(true));
    }

    #[test]
    fn blanks_and_excel_errors_are_both_null() {
        // A consumer treats "no usable value" uniformly rather than learning
        // Excel's error taxonomy.
        assert_eq!(cell_value(&Data::Empty), json!(null));
        assert_eq!(cell_value(&Data::Error(CellErrorType::Div0)), json!(null));
        assert_eq!(cell_value(&Data::Error(CellErrorType::NA)), json!(null));
    }

    #[test]
    fn dates_become_iso_8601_strings() {
        // Serial 46237 is 2026-08-03 — verified against calamine directly.
        let dt = ExcelDateTime::new(46237.0, ExcelDateTimeType::DateTime, false);
        assert_eq!(
            cell_value(&Data::DateTime(dt)),
            json!("2026-08-03T00:00:00")
        );
    }

    #[test]
    fn the_1900_epoch_boundary_matches_excels_own_arithmetic() {
        // Excel deliberately preserves a 1900-02-29 that never existed, so
        // serials on either side of it are the classic place a hand-rolled
        // converter goes wrong. These come from calamine's own mapping.
        let day_59 = ExcelDateTime::new(59.0, ExcelDateTimeType::DateTime, false);
        assert_eq!(
            cell_value(&Data::DateTime(day_59)),
            json!("1900-02-28T00:00:00")
        );

        // Serial 60 is Excel's phantom 1900-02-29; it collapses onto 02-28.
        let day_60 = ExcelDateTime::new(60.0, ExcelDateTimeType::DateTime, false);
        assert_eq!(
            cell_value(&Data::DateTime(day_60)),
            json!("1900-02-28T00:00:00")
        );

        let day_61 = ExcelDateTime::new(61.0, ExcelDateTimeType::DateTime, false);
        assert_eq!(
            cell_value(&Data::DateTime(day_61)),
            json!("1900-03-01T00:00:00")
        );
    }

    #[test]
    fn durations_render_as_exact_iso_8601() {
        let duration = |value: f64| {
            cell_value(&Data::DateTime(ExcelDateTime::new(
                value,
                ExcelDateTimeType::TimeDelta,
                false,
            )))
        };
        assert_eq!(duration(1.5625), json!("PT37H30M"));
        assert_eq!(duration(0.0), json!("PT0S"));
        assert_eq!(duration(-0.0625), json!("-PT1H30M"));
        assert_eq!(duration(1.0 / 86400.0), json!("PT1S"));
        assert_eq!(duration(0.0005 / 86400.0 * 1000.0), json!("PT0.5S"));
        assert_eq!(super::iso_duration(61_050), "PT1M1.05S");
    }

    #[test]
    fn iso_valued_cells_pass_through_unchanged() {
        assert_eq!(
            cell_value(&Data::DateTimeIso("2026-08-03T12:30:00".into())),
            json!("2026-08-03T12:30:00")
        );
        assert_eq!(cell_value(&Data::DurationIso("PT1H".into())), json!("PT1H"));
    }

    #[test]
    fn the_integer_boundary_rejects_two_to_the_sixty_three() {
        // `i64::MAX as f64` rounds UP to 2^63, so a `<=` bound admits a value
        // one larger than i64 can hold; the cast then saturates and the
        // round-trip check is fooled because casting i64::MAX back to f64
        // rounds up to 2^63 as well. Only a strict bound excludes it.
        let two_63 = 9223372036854775808.0_f64;
        assert!(
            cell_value(&Data::Float(two_63)).is_f64(),
            "2^63 does not fit i64 and must stay a float"
        );

        // The largest whole f64 that genuinely round-trips is 2^63 - 1024.
        let largest_exact = 9223372036854774784.0_f64;
        assert_eq!(
            cell_value(&Data::Float(largest_exact)),
            serde_json::json!(9223372036854774784_i64)
        );
    }
}
