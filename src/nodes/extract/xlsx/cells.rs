//! Converting one spreadsheet cell into a JSON value.

use calamine::Data;
use serde_json::Value;

/// Convert a cell to the value a flow sees.
///
/// Blanks and Excel errors both become `null`: a consumer treats "no usable
/// value" uniformly rather than learning Excel's error taxonomy. The cost is
/// that a flow auditing a workbook cannot tell a broken formula from an empty
/// cell.
#[allow(dead_code)]
pub(super) fn cell_value(cell: &Data) -> Value {
    match cell {
        Data::Int(value) => Value::from(*value),
        Data::Float(value) => number_value(*value),
        Data::String(value) => Value::String(value.clone()),
        Data::Bool(value) => Value::Bool(*value),
        // `.xlsx` has no date type — a date is a float plus a number-format
        // code. Emitting the serial would make every flow re-derive the format
        // lookup and the 1900-epoch leap-year quirk Excel preserves.
        Data::DateTime(value) => match value.as_datetime() {
            Some(datetime) => Value::String(datetime.format("%Y-%m-%dT%H:%M:%S").to_string()),
            None => Value::Null,
        },
        Data::DateTimeIso(value) | Data::DurationIso(value) => Value::String(value.clone()),
        Data::Error(_) | Data::Empty => Value::Null,
    }
}

/// Emit a whole number as an integer when it round-trips exactly.
///
/// Every `.xlsx` number is a double, so without this a quantity column reaches
/// Lua as `3.0` rather than `3` — Lua 5.4 distinguishes the two, and the noise
/// reaches every downstream comparison and interpolation.
#[allow(dead_code)]
fn number_value(value: f64) -> Value {
    if value.fract() == 0.0 && value >= i64::MIN as f64 && value <= i64::MAX as f64 {
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
    fn iso_valued_cells_pass_through_unchanged() {
        assert_eq!(
            cell_value(&Data::DateTimeIso("2026-08-03T12:30:00".into())),
            json!("2026-08-03T12:30:00")
        );
        assert_eq!(cell_value(&Data::DurationIso("PT1H".into())), json!("PT1H"));
    }
}
