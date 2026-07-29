//! Schedule declarations loaded from `ironflow.yaml`.

use std::str::FromStr;

use chrono_tz::Tz;
use cron::Schedule;
use serde::Deserialize;

use crate::engine::types::Context;

/// Number of fields in the cron dialect IronFlow accepts.
const CRON_FIELDS: usize = 5;

/// Default maximum lateness for which a missed instant still fires.
const DEFAULT_GRACE_SECONDS: u64 = 300;

/// Shortest grace window that behaves predictably against the tick interval.
pub const MIN_GRACE_SECONDS: u64 = 60;

/// Claim records are kept at least this long regardless of grace.
const MIN_CLAIM_TTL_SECONDS: u64 = 7 * 24 * 3_600;

/// Context keys IronFlow injects itself; a schedule must not preset them.
pub const RESERVED_SCHEDULE_CONTEXT_KEYS: &[&str] = &["_schedule", "_flow_dir"];

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

/// One named schedule.
#[derive(Clone, Debug)]
pub struct ScheduleConfig {
    flow: String,
    cron: Schedule,
    cron_source: String,
    timezone: Tz,
    grace_seconds: u64,
    context: Context,
}

impl ScheduleConfig {
    pub fn new(
        flow: impl Into<String>,
        cron: &str,
        timezone: Option<&str>,
        grace_seconds: Option<u64>,
        context: Context,
    ) -> Result<Self, String> {
        let flow = flow.into();
        if flow.trim().is_empty() {
            return Err("schedule flow path must not be empty".to_string());
        }

        let parsed = parse_cron(cron)?;

        let timezone = match timezone {
            None => Tz::UTC,
            Some(name) => Tz::from_str(name).map_err(|_| {
                format!("schedule timezone '{name}' is not a known IANA time zone name")
            })?,
        };

        let grace_seconds = grace_seconds.unwrap_or(DEFAULT_GRACE_SECONDS);
        if grace_seconds < MIN_GRACE_SECONDS {
            return Err(format!(
                "schedule grace_seconds must be at least {MIN_GRACE_SECONDS}; \
                 the scheduler evaluates every 30 seconds, so {grace_seconds} \
                 would skip fires unpredictably"
            ));
        }

        for reserved in RESERVED_SCHEDULE_CONTEXT_KEYS {
            if context.contains_key(*reserved) {
                return Err(format!(
                    "schedule context must not define reserved key '{reserved}'"
                ));
            }
        }

        Ok(Self {
            flow,
            cron: parsed,
            cron_source: cron.to_string(),
            timezone,
            grace_seconds,
            context,
        })
    }

    pub fn flow(&self) -> &str {
        &self.flow
    }

    pub fn cron(&self) -> &Schedule {
        &self.cron
    }

    pub fn cron_source(&self) -> &str {
        &self.cron_source
    }

    pub fn timezone(&self) -> Tz {
        self.timezone
    }

    pub fn grace_seconds(&self) -> u64 {
        self.grace_seconds
    }

    pub fn context(&self) -> &Context {
        &self.context
    }

    /// How long a claim record is retained.
    ///
    /// A claim only has to outlive the window in which its instant could still
    /// fire, so it must exceed `grace_seconds`. The one-week floor keeps the
    /// common case simple while the `+ 1 day` term keeps an unusually long
    /// grace window correct.
    pub fn claim_ttl_seconds(&self) -> u64 {
        MIN_CLAIM_TTL_SECONDS.max(self.grace_seconds.saturating_add(86_400))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawScheduleConfig {
    flow: String,
    cron: String,
    #[serde(default)]
    timezone: Option<String>,
    #[serde(default)]
    grace_seconds: Option<u64>,
    #[serde(default)]
    context: Context,
}

impl<'de> Deserialize<'de> for ScheduleConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = RawScheduleConfig::deserialize(deserializer)?;
        Self::new(
            raw.flow,
            &raw.cron,
            raw.timezone.as_deref(),
            raw.grace_seconds,
            raw.context,
        )
        .map_err(serde::de::Error::custom)
    }
}
