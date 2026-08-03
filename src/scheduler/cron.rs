//! Bounded five-field cron parsing with traditional day-field semantics.

use std::str::FromStr;

use chrono::{DateTime, Utc};
use cron::Schedule;

const CRON_FIELDS: usize = 5;

/// Maximum UTF-8 bytes accepted for one cron expression.
pub const MAX_CRON_EXPRESSION_BYTES: usize = 256;

/// A five-field cron schedule. When day-of-month and day-of-week are both
/// restricted, occurrences are the union of both fields, matching traditional
/// crontab behavior rather than the underlying parser's intersection.
#[derive(Clone, Debug)]
pub struct CronSchedule {
    alternatives: Vec<Schedule>,
}

impl CronSchedule {
    /// Iterate distinct matching wall-clock candidates strictly after `after`.
    pub fn after<'a>(&'a self, after: &DateTime<Utc>) -> impl Iterator<Item = DateTime<Utc>> + 'a {
        let mut cursor = *after;
        std::iter::from_fn(move || {
            let next = self
                .alternatives
                .iter()
                .filter_map(|schedule| schedule.after(&cursor).next())
                .min()?;
            cursor = next;
            Some(next)
        })
    }
}

/// Parse a bounded standard five-field cron expression.
pub fn parse_cron(expression: &str) -> Result<CronSchedule, String> {
    if expression.len() > MAX_CRON_EXPRESSION_BYTES {
        return Err(format!(
            "cron expression is {} bytes, exceeding the {MAX_CRON_EXPRESSION_BYTES}-byte limit",
            expression.len()
        ));
    }

    let fields = expression.split_whitespace().collect::<Vec<_>>();
    if fields.len() != CRON_FIELDS {
        return Err(format!(
            "cron expression must have five fields \
             (minute hour day month weekday), but {} were supplied in '{expression}'",
            fields.len()
        ));
    }

    let expressions = if restricted(fields[2]) && restricted(fields[4]) {
        vec![
            format!("{} {} {} {} *", fields[0], fields[1], fields[2], fields[3]),
            format!("{} {} * {} {}", fields[0], fields[1], fields[3], fields[4]),
        ]
    } else {
        vec![expression.to_string()]
    };

    let alternatives = expressions
        .into_iter()
        .map(|candidate| Schedule::from_str(&format!("0 {candidate}")))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| format!("cron expression '{expression}' is not valid"))?;

    Ok(CronSchedule { alternatives })
}

fn restricted(field: &str) -> bool {
    !matches!(field, "*" | "?")
}
