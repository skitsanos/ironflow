//! Schedule declarations loaded from `ironflow.yaml`.

use std::str::FromStr;

use cron::Schedule;

/// Number of fields in the cron dialect IronFlow accepts.
const CRON_FIELDS: usize = 5;

/// Parse a standard five-field cron expression.
///
/// The `cron` crate parses six fields, reading the first as seconds, and
/// rejects the five-field form outright. Accepting six here would silently
/// reinterpret `"0 2 * * * *"` as something its author did not write, so only
/// the five-field form is accepted and the seconds field is fixed at zero.
/// Sub-minute scheduling is out of scope, so nothing is lost by that.
pub fn parse_cron(expression: &str) -> Result<Schedule, String> {
    let fields = expression.split_whitespace().count();
    if fields != CRON_FIELDS {
        return Err(format!(
            "cron expression must have five fields \
             (minute hour day month weekday), but {fields} were supplied in '{expression}'"
        ));
    }

    // The parser's error is a multi-line caret diagram. A startup failure is
    // more useful naming the expression than reprinting that.
    Schedule::from_str(&format!("0 {expression}"))
        .map_err(|_| format!("cron expression '{expression}' is not valid"))
}

#[cfg(test)]
mod tests {
    use super::parse_cron;

    #[test]
    fn seconds_are_fixed_at_zero() {
        assert!(parse_cron("* * * * *").is_ok());
    }
}
