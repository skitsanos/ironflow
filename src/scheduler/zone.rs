//! Named time zones for schedules.
//!
//! Zone rules come from the IANA Time Zone Database compiled into the binary
//! (`jiff-tzdb`), never from the host: replicas compare claim keys, so every
//! replica of one release must agree on every zone's rules, including on
//! Windows and in images that ship no `/usr/share/zoneinfo`.
//!
//! `chrono` remains the type at the `cron` boundary (IF-231 migrates in
//! stages), so this module converts at its edges and exposes nothing of `jiff`.

use std::sync::Arc;

use chrono::{
    DateTime, Datelike, FixedOffset, NaiveDateTime, Offset as _, TimeZone as _, Timelike, Utc,
};
use jiff::tz::{AmbiguousOffset, Offset, TimeZone, TimeZoneDatabase};

/// One IANA time zone, identified by the exact name it was configured with.
#[derive(Clone, Debug)]
pub struct ScheduleZone {
    name: Arc<str>,
    rules: TimeZone,
}

impl ScheduleZone {
    /// Coordinated Universal Time, the default for a schedule.
    pub fn utc() -> Self {
        Self {
            name: Arc::from("UTC"),
            rules: TimeZone::UTC,
        }
    }

    /// Look up an IANA name such as `Europe/Berlin`.
    ///
    /// The name must be spelled exactly as the database spells it. The lookup
    /// itself ignores ASCII case, but the name is part of every claim key, so
    /// two spellings of one zone would claim the same fire twice.
    pub fn named(name: &str) -> Result<Self, String> {
        let unknown = || format!("schedule timezone '{name}' is not a known IANA time zone name");
        let rules = TimeZoneDatabase::bundled()
            .get(name)
            .map_err(|_| unknown())?;
        if rules.is_unknown() || rules.iana_name() != Some(name) {
            return Err(unknown());
        }
        Ok(Self {
            name: Arc::from(name),
            rules,
        })
    }

    /// The IANA name, as written into claim keys.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The earliest instant at which this zone's wall clock shows `local`.
    ///
    /// A wall-clock time that repeats when clocks go back has two instants and
    /// yields the earlier. One that is skipped when clocks go forward has none.
    pub fn earliest_instant(&self, local: NaiveDateTime) -> Option<DateTime<FixedOffset>> {
        let offset = match self.rules.to_ambiguous_timestamp(civil(local)?).offset() {
            AmbiguousOffset::Unambiguous { offset } => offset,
            AmbiguousOffset::Fold { before, .. } => before,
            AmbiguousOffset::Gap { .. } => return None,
        };
        fixed(offset)?.from_local_datetime(&local).single()
    }

    /// The wall-clock time this zone shows at `instant`.
    pub fn local_time(&self, instant: DateTime<Utc>) -> NaiveDateTime {
        // `jiff` covers the years -9999 to 9999. Beyond them the rules of the
        // nearest covered instant apply, as a zone's last rule does.
        let second = instant.timestamp().clamp(
            jiff::Timestamp::MIN.as_second(),
            jiff::Timestamp::MAX.as_second(),
        );
        let offset = jiff::Timestamp::from_second(second)
            .ok()
            .and_then(|timestamp| fixed(self.rules.to_offset(timestamp)))
            .unwrap_or_else(|| Utc.fix());
        instant.with_timezone(&offset).naive_local()
    }
}

impl PartialEq for ScheduleZone {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

impl Eq for ScheduleZone {}

fn fixed(offset: Offset) -> Option<FixedOffset> {
    FixedOffset::east_opt(offset.seconds())
}

/// `None` for a time outside the years `jiff` covers, or on a leap second.
fn civil(local: NaiveDateTime) -> Option<jiff::civil::DateTime> {
    let narrow = |value: u32| i8::try_from(value).ok();
    jiff::civil::DateTime::new(
        i16::try_from(local.year()).ok()?,
        narrow(local.month())?,
        narrow(local.day())?,
        narrow(local.hour())?,
        narrow(local.minute())?,
        narrow(local.second())?,
        i32::try_from(local.nanosecond()).ok()?,
    )
    .ok()
}
