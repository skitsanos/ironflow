//! Named time zones for schedules.
//!
//! Zone rules come from the IANA Time Zone Database compiled into the binary
//! (`jiff-tzdb`), never from the host: replicas compare claim keys, so every
//! replica of one release must agree on every zone's rules, including on
//! Windows and in images that ship no `/usr/share/zoneinfo`.
//!
//! Wall-clock times are `jiff::civil::DateTime` and instants are
//! `jiff::Timestamp` or `jiff::ZonedDateTime`: a wall-clock time has no zone
//! and cannot be mistaken for an instant.

use std::sync::Arc;

use jiff::tz::{AmbiguousOffset, TimeZone, TimeZoneDatabase};
use jiff::{Timestamp, ZonedDateTime, civil};

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
    /// yields the earlier. One that is skipped when clocks go forward has
    /// none, and neither has one whose instant lies outside the years `jiff`
    /// covers.
    pub fn earliest_instant(&self, local: civil::DateTime) -> Option<ZonedDateTime> {
        match self.resolve(local) {
            WallClock::At(instant) => Some(instant),
            WallClock::Skipped | WallClock::OutOfRange => None,
        }
    }

    /// `earliest_instant`, telling a skipped wall-clock time from one that is
    /// out of range: only the first has a next instant worth looking for.
    pub(super) fn resolve(&self, local: civil::DateTime) -> WallClock {
        let offset = match self.rules.to_ambiguous_timestamp(local).offset() {
            AmbiguousOffset::Unambiguous { offset } => offset,
            AmbiguousOffset::Fold { before, .. } => before,
            AmbiguousOffset::Gap { .. } => return WallClock::Skipped,
        };
        match offset.to_timestamp(local) {
            Ok(instant) => WallClock::At(instant.to_zoned_datetime(self.rules.clone())),
            Err(_) => WallClock::OutOfRange,
        }
    }

    /// The wall-clock time this zone shows at `instant`.
    pub fn local_time(&self, instant: Timestamp) -> civil::DateTime {
        self.rules.to_datetime(instant)
    }
}

/// What a zone's wall clock makes of one wall-clock time.
pub(super) enum WallClock {
    /// The clock shows it, first at this instant.
    At(ZonedDateTime),
    /// The clock never shows it: clocks went forward over it.
    Skipped,
    /// Its instant is before the first or after the last one `jiff`
    /// represents (the years -9999 to 9999, less 26 hours at each end).
    OutOfRange,
}

impl PartialEq for ScheduleZone {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

impl Eq for ScheduleZone {}
