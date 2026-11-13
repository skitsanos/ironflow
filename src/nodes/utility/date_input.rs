//! Offset-aware parsing and panic-free formatting for the `date_format` node.
//!
//! Both format strings are untrusted. chrono's `Display` adapter fails on an
//! invalid item and `to_string()` turns that failure into a panic, and its
//! naive parsers accept offset specifiers but discard the parsed offset
//! (IF-195). Everything here returns an error instead.

use std::fmt::Write as _;

use anyhow::{Result, anyhow, bail};
use chrono::format::{Fixed, Item, Parsed, StrftimeItems, parse, parse_and_remainder};
use chrono::{DateTime, FixedOffset, NaiveTime, TimeZone as _};

/// Zone names `%Z` accepts on input. chrono skips the name without reading
/// it, so any other name would silently be treated as UTC.
const UTC_ZONE_NAMES: [&str; 4] = ["UTC", "GMT", "UT", "Z"];

/// Parse `input` with a user-supplied strftime `format`.
///
/// A parsed numeric offset (`%z`, `%:z`, `%#z`, `%+`) is applied and kept on
/// the result. Without one the value is UTC, as before; a `%Z` zone name is
/// accepted only when it names UTC. A date without a time is midnight.
pub(super) fn parse_with_format(input: &str, format: &str) -> Result<DateTime<FixedOffset>> {
    let failed = |e: &dyn std::fmt::Display| {
        anyhow!("Failed to parse '{input}' with format '{format}': {e}")
    };
    let items = StrftimeItems::new(format).parse().map_err(|e| failed(&e))?;
    let mut parsed = Parsed::new();
    parse(&mut parsed, input, items.iter()).map_err(|e| failed(&e))?;

    let offset_seconds = match parsed.offset() {
        Some(seconds) => seconds,
        None => {
            reject_ignored_zone_names(input, format, &items)?;
            0
        }
    };
    let naive = parsed
        .to_naive_datetime_with_offset(offset_seconds)
        .or_else(|_| parsed.to_naive_date().map(|d| d.and_time(NaiveTime::MIN)))
        .map_err(|e| failed(&e))?;
    FixedOffset::east_opt(offset_seconds)
        .and_then(|offset| offset.from_local_datetime(&naive).single())
        .ok_or_else(|| failed(&"offset out of range"))
}

/// Fail when a `%Z` item consumed a zone name that is not UTC.
fn reject_ignored_zone_names(input: &str, format: &str, items: &[Item<'_>]) -> Result<()> {
    for (index, item) in items.iter().enumerate() {
        if *item != Item::Fixed(Fixed::TimezoneName) {
            continue;
        }
        // The parser is sequential, so replaying the items before `%Z`
        // leaves the zone name at the start of the remainder.
        let mut scratch = Parsed::new();
        let rest = parse_and_remainder(&mut scratch, input, items[..index].iter())
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

/// Format `dt` with a user-supplied strftime `format` without panicking.
pub(super) fn format_output(dt: &DateTime<FixedOffset>, format: &str) -> Result<String> {
    let items = StrftimeItems::new(format).parse().map_err(|_| {
        anyhow!(
            "date_format: invalid output_format '{format}': unknown or incomplete strftime \
             specifier"
        )
    })?;
    // Valid items can still fail to format: `%#z` is parse-only.
    let mut formatted = String::new();
    write!(formatted, "{}", dt.format_with_items(items.iter())).map_err(|_| {
        anyhow!(
            "date_format: invalid output_format '{format}': it has a parse-only specifier \
             such as %#z, or cannot represent this date"
        )
    })?;
    Ok(formatted)
}

/// Seconds east of UTC for `sign * (hours:minutes)`, or `None` on overflow.
pub(super) fn offset_seconds(sign: i32, hours: i32, minutes: i32) -> Option<i32> {
    hours
        .checked_mul(3600)?
        .checked_add(minutes.checked_mul(60)?)?
        .checked_mul(sign)
}
