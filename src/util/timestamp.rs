//! The text and integer forms of a stored or published instant.
//!
//! IronFlow's records, wire payloads and catalogs were first written with
//! `chrono`. An instant is now a [`jiff::Timestamp`], whose own text form
//! trims the fraction to its shortest spelling and whose integer conversions
//! round toward zero, so every form is produced here instead and stays byte
//! for byte what it was (IF-231):
//!
//! | form | example | used by |
//! | --- | --- | --- |
//! | [`rfc3339_z`] | `2026-01-02T03:04:05.120Z` | JSON and Redis records, API bodies, SSE payloads, run cursors |
//! | [`rfc3339_offset`] | `2026-01-02T03:04:05.120+00:00` | SQL event rows, Lua `now_rfc3339()` |
//! | [`rfc3339_nanos_z`] | `2026-01-02T03:04:05.120000000Z` | SQL run and task columns |
//! | [`unix_micros`] | `1767323045120000` | run catalogs, leases, schedule claims |
//!
//! The first two write no fraction for a whole second and otherwise three,
//! six or nine digits, whichever is the shortest that loses nothing. A replica
//! that still runs the `chrono` code reads all of them and orders them the
//! same way.
//!
//! [`parse_rfc3339`] reads every one of these forms from either
//! implementation. `jiff` holds the years -9999 to 9999 (to the second, from
//! -9999-01-02T01:59:59Z to 9999-12-30T22:00:00.999999999Z); an instant
//! outside them cannot be read or written.

use std::fmt::Write as _;

use jiff::Timestamp;
use jiff::tz::Offset;

/// How many fraction digits a text form carries.
#[derive(Clone, Copy)]
enum Fraction {
    /// None, three, six or nine: the shortest of those that is exact.
    Shortest,
    /// Always nine.
    Nanos,
}

/// The form `serde` gave a `chrono` UTC time: shortest fraction, `Z`.
pub fn rfc3339_z(instant: Timestamp) -> String {
    write_rfc3339(instant, Fraction::Shortest, "Z")
}

/// The form `chrono`'s `to_rfc3339` gave: shortest fraction, `+00:00`.
pub fn rfc3339_offset(instant: Timestamp) -> String {
    write_rfc3339(instant, Fraction::Shortest, "+00:00")
}

/// Nine fraction digits and `Z`.
pub fn rfc3339_nanos_z(instant: Timestamp) -> String {
    write_rfc3339(instant, Fraction::Nanos, "Z")
}

fn write_rfc3339(instant: Timestamp, fraction: Fraction, offset: &str) -> String {
    let utc = Offset::UTC.to_datetime(instant);
    let mut text = String::with_capacity(35);
    // Writing to a `String` cannot fail.
    let year = utc.year();
    let _ = if (0..=9999).contains(&year) {
        write!(text, "{year:04}")
    } else {
        // ISO 8601 signs a year outside four digits; `jiff` ends at -9999.
        write!(text, "{year:+05}")
    };
    let _ = write!(
        text,
        "-{:02}-{:02}T{:02}:{:02}:{:02}",
        utc.month(),
        utc.day(),
        utc.hour(),
        utc.minute(),
        utc.second()
    );
    let nanos = utc.subsec_nanosecond();
    let _ = match fraction {
        Fraction::Nanos => write!(text, ".{nanos:09}"),
        Fraction::Shortest if nanos == 0 => Ok(()),
        Fraction::Shortest if nanos % 1_000_000 == 0 => {
            write!(text, ".{:03}", nanos / 1_000_000)
        }
        Fraction::Shortest if nanos % 1_000 == 0 => write!(text, ".{:06}", nanos / 1_000),
        Fraction::Shortest => write!(text, ".{nanos:09}"),
    };
    text.push_str(offset);
    text
}

/// Read an RFC 3339 instant in any form IronFlow has written, with `Z` or a
/// numeric offset and any number of fraction digits up to nine.
pub fn parse_rfc3339(text: &str) -> Result<Timestamp, jiff::Error> {
    // `chrono` wrote a year before 0000 as a sign and four digits; `jiff`
    // reads a signed year only with six.
    let bytes = text.as_bytes();
    if bytes.len() > 5
        && bytes[0] == b'-'
        && bytes[1..5].iter().all(u8::is_ascii_digit)
        && bytes[5] == b'-'
    {
        return format!("-00{}", &text[1..]).parse();
    }
    text.parse()
}

/// Whole microseconds since the Unix epoch, rounded down.
///
/// Catalogs, leases and claims store and compare this number. Rounding down
/// rather than toward zero keeps the order of instants before 1970 and
/// matches what was stored before.
pub fn unix_micros(instant: Timestamp) -> i64 {
    // The whole `jiff` range is below 2^59 microseconds.
    i64::try_from(instant.as_nanosecond().div_euclid(1_000)).unwrap_or(i64::MAX)
}

/// Whole milliseconds since the Unix epoch, rounded down.
pub fn unix_millis(instant: Timestamp) -> i64 {
    i64::try_from(instant.as_nanosecond().div_euclid(1_000_000)).unwrap_or(i64::MAX)
}

/// The instant `micros` microseconds after the Unix epoch, or `None` outside
/// the supported years.
pub fn from_unix_micros(micros: i64) -> Option<Timestamp> {
    // Older Jiff 1.0 releases accepted by Cargo.toml rejected fractions of the
    // final second. Use the same conversion across the supported versions.
    let nanos = i32::try_from(micros.rem_euclid(1_000_000) * 1_000).ok()?;
    Timestamp::new(micros.div_euclid(1_000_000), nanos).ok()
}

/// Whole milliseconds from `started` to `finished`, rounded toward zero, or
/// `None` when `finished` is the earlier one.
pub fn elapsed_millis(started: Timestamp, finished: Timestamp) -> Option<u64> {
    u64::try_from(finished.duration_since(started).as_millis()).ok()
}

/// `#[serde(with = "...")]` for a required instant in the [`rfc3339_z`] form.
pub mod serde_instant {
    use jiff::Timestamp;
    use serde::{Deserialize as _, Deserializer, Serializer, de};

    pub fn serialize<S: Serializer>(instant: &Timestamp, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&super::rfc3339_z(*instant))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Timestamp, D::Error> {
        let text = String::deserialize(deserializer)?;
        super::parse_rfc3339(&text).map_err(de::Error::custom)
    }
}

/// `#[serde(default, with = "...")]` for an optional instant: `null` or an
/// absent key is `None`.
pub mod serde_option {
    use jiff::Timestamp;
    use serde::{Deserialize as _, Deserializer, Serializer, de};

    pub fn serialize<S: Serializer>(
        instant: &Option<Timestamp>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        match instant {
            Some(instant) => serializer.serialize_some(&super::rfc3339_z(*instant)),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<Timestamp>, D::Error> {
        Option::<String>::deserialize(deserializer)?
            .map(|text| super::parse_rfc3339(&text).map_err(de::Error::custom))
            .transpose()
    }
}

#[cfg(test)]
#[path = "timestamp_tests.rs"]
mod tests;
