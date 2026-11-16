//! Startup check of the Redis record TTL against schedule grace windows
//! (IF-201).
//!
//! A scheduled occurrence can still be evaluated until its grace window
//! closes. With `REDIS_TTL` at or below that window, the run record of a
//! finished occurrence expires while a restarted replica can still reach the
//! instant. The claim's own "run created" record keeps such an instant from
//! running twice, which is why this is a warning and not a startup failure:
//! the configuration is safe, and rejecting it would stop deployments that
//! run correctly. What a short TTL still costs is stated in the message.

use std::collections::HashMap;

use anyhow::Result;

use crate::scheduler::config::ScheduleConfig;

use super::IronFlowConfig;
use super::presence::environment_non_blank;
use super::resolution::environment_value;

/// Slack between a grace window and the record TTL that covers it: one
/// scheduler tick plus one evaluation budget, rounded up, so a replica's last
/// in-grace evaluation of an instant still finds the record.
const RECORD_TTL_MARGIN_SECONDS: u64 = 60;

/// One schedule whose grace window the Redis record TTL does not cover.
#[derive(Debug, PartialEq, Eq)]
struct ShortRetention {
    schedule: String,
    grace_seconds: u64,
    /// Smallest `REDIS_TTL` that covers this schedule.
    minimum_ttl_seconds: u64,
}

/// Log one warning per schedule whose run records can expire inside its
/// grace window.
pub(crate) fn warn_about_short_redis_ttl(
    cfg: &IronFlowConfig,
    schedules: &HashMap<String, ScheduleConfig>,
) -> Result<()> {
    let backend = environment_non_blank("IRONFLOW_STORE")?;
    let ttl = environment_value("REDIS_TTL", "an unsigned integer")?;
    let Some((ttl, uncovered)) = uncovered_schedules(backend, ttl, cfg, schedules) else {
        return Ok(());
    };
    for short in uncovered {
        tracing::warn!(
            schedule = %short.schedule,
            grace_seconds = short.grace_seconds,
            redis_ttl_seconds = ttl,
            minimum_redis_ttl_seconds = short.minimum_ttl_seconds,
            "REDIS_TTL does not cover this schedule's grace window: run records can \
             expire while the occurrence is still reachable. The occurrence is not run \
             again, but its record and history disappear early, and a replica that dies \
             between creating a run and recording it is no longer covered. Raise \
             REDIS_TTL to at least the minimum, or lower grace_seconds"
        );
    }
    Ok(())
}

/// The effective Redis record TTL and the schedules it does not cover, from
/// the already-read environment values and their configuration fallbacks.
/// `None` when records do not expire: only the Redis state store expires
/// run records, and only with a TTL.
fn uncovered_schedules(
    environment_backend: Option<String>,
    environment_ttl: Option<u64>,
    cfg: &IronFlowConfig,
    schedules: &HashMap<String, ScheduleConfig>,
) -> Option<(u64, Vec<ShortRetention>)> {
    let backend = environment_backend
        .or_else(|| cfg.store_backend.clone())
        .unwrap_or_else(|| super::store_factory::DEFAULT_STATE_BACKEND.to_string());
    if backend != "redis" {
        return None;
    }
    let ttl = environment_ttl.or(cfg.redis_ttl)?;
    Some((ttl, short_retention(ttl, schedules)))
}

/// Schedules, sorted by name, whose grace window plus margin is not strictly
/// below `ttl_seconds`.
fn short_retention(
    ttl_seconds: u64,
    schedules: &HashMap<String, ScheduleConfig>,
) -> Vec<ShortRetention> {
    let mut short: Vec<ShortRetention> = schedules
        .iter()
        .filter_map(|(name, schedule)| {
            let covered = schedule
                .grace_seconds()
                .saturating_add(RECORD_TTL_MARGIN_SECONDS);
            (ttl_seconds <= covered).then(|| ShortRetention {
                schedule: name.clone(),
                grace_seconds: schedule.grace_seconds(),
                minimum_ttl_seconds: covered.saturating_add(1),
            })
        })
        .collect();
    short.sort_by(|left, right| left.schedule.cmp(&right.schedule));
    short
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::{
        IronFlowConfig, RECORD_TTL_MARGIN_SECONDS, ShortRetention, short_retention,
        uncovered_schedules,
    };
    use crate::engine::types::Context;
    use crate::scheduler::config::ScheduleConfig;

    fn schedule(grace_seconds: u64) -> ScheduleConfig {
        ScheduleConfig::new(
            "nightly.lua",
            "0 2 * * *",
            None,
            Some(grace_seconds),
            Context::new(),
        )
        .unwrap()
    }

    #[test]
    fn the_finding_configuration_is_reported() {
        // `grace_seconds: 86400` with `REDIS_TTL=3600`, as in IF-201.
        let schedules = HashMap::from([("nightly".to_string(), schedule(86_400))]);

        assert_eq!(
            short_retention(3_600, &schedules),
            vec![ShortRetention {
                schedule: "nightly".to_string(),
                grace_seconds: 86_400,
                minimum_ttl_seconds: 86_400 + RECORD_TTL_MARGIN_SECONDS + 1,
            }]
        );
    }

    #[test]
    fn the_ttl_must_exceed_grace_plus_the_margin() {
        let schedules = HashMap::from([("nightly".to_string(), schedule(300))]);
        let covered = 300 + RECORD_TTL_MARGIN_SECONDS;

        assert_eq!(short_retention(300, &schedules).len(), 1);
        assert_eq!(short_retention(covered, &schedules).len(), 1);
        assert!(short_retention(covered + 1, &schedules).is_empty());
    }

    #[test]
    fn only_uncovered_schedules_are_reported_in_name_order() {
        let schedules = HashMap::from([
            ("weekly".to_string(), schedule(604_800)),
            ("hourly".to_string(), schedule(60)),
            ("daily".to_string(), schedule(86_400)),
        ]);

        let names: Vec<String> = short_retention(7_200, &schedules)
            .into_iter()
            .map(|short| short.schedule)
            .collect();

        assert_eq!(names, ["daily", "weekly"]);
    }

    fn redis_config(ttl: Option<u64>) -> IronFlowConfig {
        IronFlowConfig {
            store_backend: Some("redis".to_string()),
            redis_ttl: ttl,
            ..IronFlowConfig::default()
        }
    }

    #[test]
    fn only_a_redis_state_store_with_a_ttl_is_checked() {
        let schedules = HashMap::from([("nightly".to_string(), schedule(86_400))]);

        // SQLite is the default backend, and SQL and JSON records never expire.
        let default = IronFlowConfig {
            redis_ttl: Some(3_600),
            ..IronFlowConfig::default()
        };
        assert_eq!(uncovered_schedules(None, None, &default, &schedules), None);
        // Redis without a TTL keeps records forever.
        assert_eq!(
            uncovered_schedules(None, None, &redis_config(None), &schedules),
            None
        );

        let (ttl, uncovered) =
            uncovered_schedules(None, None, &redis_config(Some(3_600)), &schedules).unwrap();
        assert_eq!(ttl, 3_600);
        assert_eq!(uncovered.len(), 1);
    }

    #[test]
    fn environment_values_override_the_configuration_file() {
        let schedules = HashMap::from([("nightly".to_string(), schedule(86_400))]);

        // `IRONFLOW_STORE` selects Redis over the configured backend.
        let sqlite = IronFlowConfig {
            store_backend: Some("sqlite".to_string()),
            redis_ttl: Some(3_600),
            ..IronFlowConfig::default()
        };
        let (_, uncovered) =
            uncovered_schedules(Some("redis".to_string()), None, &sqlite, &schedules).unwrap();
        assert_eq!(uncovered.len(), 1);
        assert_eq!(
            uncovered_schedules(
                Some("postgres".to_string()),
                None,
                &redis_config(Some(3_600)),
                &schedules
            ),
            None
        );

        // `REDIS_TTL` replaces a configured `redis_ttl`, in both directions.
        let (ttl, uncovered) =
            uncovered_schedules(None, Some(172_800), &redis_config(Some(3_600)), &schedules)
                .unwrap();
        assert_eq!((ttl, uncovered.len()), (172_800, 0));
        let (ttl, uncovered) =
            uncovered_schedules(None, Some(3_600), &redis_config(Some(172_800)), &schedules)
                .unwrap();
        assert_eq!((ttl, uncovered.len()), (3_600, 1));
    }
}
