//! Schedule declarations loaded from `ironflow.yaml`.

use chrono_tz::Tz;
use serde::Deserialize;

use crate::engine::types::Context;

pub use super::cron::{CronSchedule, MAX_CRON_EXPRESSION_BYTES, parse_cron};

/// Maximum number of configured schedule names.
pub const MAX_SCHEDULES: usize = 256;

/// Maximum UTF-8 bytes in a schedule name. This keeps JSON claim filenames
/// below common 255-byte component limits without changing their durable key
/// format during rolling upgrades.
pub const MAX_SCHEDULE_NAME_BYTES: usize = 48;

/// Maximum UTF-8 bytes in a configured flow path.
pub const MAX_SCHEDULE_FLOW_BYTES: usize = 1_024;

/// Maximum UTF-8 bytes in an IANA timezone name.
pub const MAX_SCHEDULE_TIMEZONE_BYTES: usize = 128;

/// Maximum serialized JSON bytes in one schedule's initial context.
pub const MAX_SCHEDULE_CONTEXT_BYTES: usize = 64 * 1_024;

/// Default maximum lateness for which a missed instant still fires.
const DEFAULT_GRACE_SECONDS: u64 = 300;

/// Shortest grace window that behaves predictably against the tick interval.
pub const MIN_GRACE_SECONDS: u64 = 60;

/// Longest catch-up window accepted from configuration.
pub const MAX_GRACE_SECONDS: u64 = 7 * 24 * 3_600;

/// Claim records are kept at least this long regardless of grace.
const MIN_CLAIM_TTL_SECONDS: u64 = 7 * 24 * 3_600;

/// Context keys IronFlow injects itself; a schedule must not preset them.
pub const RESERVED_SCHEDULE_CONTEXT_KEYS: &[&str] = &["_schedule", "_flow_dir"];

/// Validate a schedule map key before it reaches logs, context, or storage.
pub fn validate_schedule_name(name: &str) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err("schedule name must not be empty".to_string());
    }
    if name.len() > MAX_SCHEDULE_NAME_BYTES {
        return Err(format!(
            "schedule name is {} bytes, exceeding the {MAX_SCHEDULE_NAME_BYTES}-byte limit",
            name.len()
        ));
    }
    if name.chars().any(char::is_control) {
        return Err("schedule name must not contain control characters".to_string());
    }
    Ok(())
}

/// One named schedule.
#[derive(Clone, Debug)]
pub struct ScheduleConfig {
    flow: String,
    cron: CronSchedule,
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
        if flow.len() > MAX_SCHEDULE_FLOW_BYTES {
            return Err(format!(
                "schedule flow path is {} bytes, exceeding the {MAX_SCHEDULE_FLOW_BYTES}-byte limit",
                flow.len()
            ));
        }

        let parsed = parse_cron(cron)?;

        let timezone = match timezone {
            None => Tz::UTC,
            Some(name) => {
                if name.len() > MAX_SCHEDULE_TIMEZONE_BYTES {
                    return Err(format!(
                        "schedule timezone is {} bytes, exceeding the \
                         {MAX_SCHEDULE_TIMEZONE_BYTES}-byte limit",
                        name.len()
                    ));
                }
                name.parse::<Tz>().map_err(|_| {
                    format!("schedule timezone '{name}' is not a known IANA time zone name")
                })?
            }
        };

        let grace_seconds = grace_seconds.unwrap_or(DEFAULT_GRACE_SECONDS);
        if grace_seconds < MIN_GRACE_SECONDS {
            return Err(format!(
                "schedule grace_seconds must be at least {MIN_GRACE_SECONDS}; \
                 the scheduler evaluates every 30 seconds, so {grace_seconds} \
                 would skip fires unpredictably"
            ));
        }
        if grace_seconds > MAX_GRACE_SECONDS {
            return Err(format!(
                "schedule grace_seconds must not exceed {MAX_GRACE_SECONDS}; \
                 {grace_seconds} was supplied"
            ));
        }

        for reserved in RESERVED_SCHEDULE_CONTEXT_KEYS {
            if context.contains_key(*reserved) {
                return Err(format!(
                    "schedule context must not define reserved key '{reserved}'"
                ));
            }
        }
        let context_bytes = serde_json::to_vec(&context)
            .map_err(|error| format!("schedule context could not be serialized: {error}"))?
            .len();
        if context_bytes > MAX_SCHEDULE_CONTEXT_BYTES {
            return Err(format!(
                "schedule context is {context_bytes} bytes, exceeding the \
                 {MAX_SCHEDULE_CONTEXT_BYTES}-byte limit"
            ));
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

    pub fn cron(&self) -> &CronSchedule {
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

    pub(super) fn grace_seconds_i64(&self) -> i64 {
        i64::try_from(self.grace_seconds).expect("validated grace fits i64")
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
