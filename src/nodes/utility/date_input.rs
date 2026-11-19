//! Reading the input and writing the result of the `date_format` node.
//!
//! Both format strings are untrusted: a bad one is an error, never a panic.
//! A parsed numeric offset is applied and kept (IF-195).

use anyhow::{Result, anyhow, bail};
use jiff::civil::{DateTime, Time};

use super::strftime::{self, Fields, Fixed, Item, Moment, ParseError};

/// Zone names `%Z` accepts on input. The name is skipped without being
/// read, so any other name would silently be treated as UTC.
const UTC_ZONE_NAMES: [&str; 4] = ["UTC", "GMT", "UT", "Z"];

/// Formats tried, in order, for an input with no offset. It is read as UTC.
const NAIVE_FORMATS: [&str; 3] = ["%Y-%m-%d %H:%M:%S", "%Y-%m-%dT%H:%M:%S", "%Y-%m-%d"];

/// Seconds east of UTC that a value may carry: strictly within one day.
fn within_one_day(seconds: i32) -> Option<i32> {
    (-86_399..=86_399).contains(&seconds).then_some(seconds)
}

/// Read all of `input` with `items`.
fn read(input: &str, items: &[Item<'_>]) -> Result<Fields, ParseError> {
    let mut fields = Fields::default();
    match strftime::parse_prefix(&mut fields, input, items)? {
        "" => Ok(fields),
        _ => Err(ParseError::TooLong),
    }
}

/// The wall-clock time in `fields` at `offset`; a date alone is midnight.
fn resolve(fields: &Fields, offset: i32) -> Result<(DateTime, bool), ParseError> {
    if fields.is_date_only() {
        return Ok((
            DateTime::from_parts(fields.date()?, Time::midnight()),
            false,
        ));
    }
    let resolved = fields.date_time(offset)?;
    Ok((resolved.local, resolved.leap))
}

/// Try the formats the node detects without an `input_format`.
pub(super) fn auto_parse(input: &str) -> Option<Moment> {
    if let Ok(moment) = strftime::parse_rfc3339(input) {
        return Some(moment);
    }
    if let Ok(moment) = strftime::parse_rfc2822(input) {
        return Some(moment);
    }
    NAIVE_FORMATS.iter().find_map(|format| {
        let fields = read(input, &strftime::items(format)?).ok()?;
        // The last format has no time; the others must have one.
        let (local, leap) = if format.contains("%H") {
            let resolved = fields.date_time(0).ok()?;
            (resolved.local, resolved.leap)
        } else {
            (
                DateTime::from_parts(fields.date().ok()?, Time::midnight()),
                false,
            )
        };
        Some(Moment {
            local,
            offset: 0,
            leap,
        })
    })
}

/// Parse `input` with a user-supplied strftime `format`.
///
/// A parsed numeric offset (`%z`, `%:z`, `%#z`, `%+`) is applied and kept on
/// the result. Without one the value is UTC; a `%Z` zone name is accepted
/// only when it names UTC. A date without a time is midnight.
pub(super) fn parse_with_format(input: &str, format: &str) -> Result<Moment> {
    let failed = |e: &dyn std::fmt::Display| {
        anyhow!("Failed to parse '{input}' with format '{format}': {e}")
    };
    let items = strftime::items(format).ok_or_else(|| failed(&ParseError::BadFormat))?;
    let fields = read(input, &items).map_err(|e| failed(&e))?;

    let offset = match fields.offset() {
        Some(seconds) => seconds,
        None => {
            reject_ignored_zone_names(input, format, &items)?;
            0
        }
    };
    let (local, leap) = resolve(&fields, offset).map_err(|e| failed(&e))?;
    let offset = within_one_day(offset).ok_or_else(|| failed(&"offset out of range"))?;
    Ok(Moment {
        local,
        offset,
        leap,
    })
}

/// Fail when a `%Z` item consumed a zone name that is not UTC.
fn reject_ignored_zone_names(input: &str, format: &str, items: &[Item<'_>]) -> Result<()> {
    for (index, item) in items.iter().enumerate() {
        if *item != Item::Fixed(Fixed::ZoneName) {
            continue;
        }
        // The parser is sequential, so replaying the items before `%Z`
        // leaves the zone name at the start of the remainder.
        let rest = strftime::parse_prefix(&mut Fields::default(), input, &items[..index])
            .map_err(|e| anyhow!("Failed to parse '{input}' with format '{format}': {e}"))?;
        let name = rest.split(char::is_whitespace).next().unwrap_or_default();
        let is_utc = UTC_ZONE_NAMES
            .iter()
            .any(|known| name.eq_ignore_ascii_case(known));
        if !name.is_empty() && !is_utc {
            bail!(
                "date_format: input_format '{format}' reads the zone name '{name}' with %Z, \
                 which cannot be converted to an offset; only UTC, GMT, UT and Z are accepted. \
                 Use a numeric offset specifier such as %z or %:z"
            );
        }
    }
    Ok(())
}

/// Format `moment` with a user-supplied strftime `format`.
pub(super) fn format_output(moment: &Moment, format: &str) -> Result<String> {
    let items = strftime::items(format).ok_or_else(|| {
        anyhow!(
            "date_format: invalid output_format '{format}': unknown or incomplete strftime \
             specifier"
        )
    })?;
    // Valid items can still fail to format: `%#z` is parse-only.
    strftime::format(moment, &items).ok_or_else(|| {
        anyhow!(
            "date_format: invalid output_format '{format}': it has a parse-only specifier \
             such as %#z, or cannot represent this date"
        )
    })
}

/// Parse a `timezone` value like "+02:00", "-05:00", "UTC" or "Z" into
/// seconds east of UTC.
pub(super) fn parse_timezone(tz: &str) -> Result<i32> {
    let trimmed = tz.trim();
    if trimmed.eq_ignore_ascii_case("utc") || trimmed == "Z" {
        return Ok(0);
    }
    if let Some(seconds) = strftime::strict_numeric_offset(trimmed).and_then(within_one_day) {
        return Ok(seconds);
    }
    bail!(
        "Invalid timezone '{}'. Use 'UTC', '+HH:MM', or '-HH:MM'",
        tz
    )
}
