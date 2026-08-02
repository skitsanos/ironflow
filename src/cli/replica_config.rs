use anyhow::Result;

use super::IronFlowConfig;
use super::resolution::environment_string;

pub(crate) fn validate(cfg: &IronFlowConfig, replica_mode: bool) -> Result<()> {
    if !replica_mode {
        return Ok(());
    }
    let state = environment_string("IRONFLOW_STORE")?
        .or_else(|| cfg.store_backend.clone())
        .unwrap_or_else(|| "json".to_string());
    let events = environment_string("IRONFLOW_EVENT_STORE")?
        .or_else(|| cfg.event_store.clone())
        .unwrap_or_else(|| "memory".to_string());
    validate_backends(&state, &events)
}

fn validate_backends(state: &str, events: &str) -> Result<()> {
    if !matches!(state, "postgres" | "redis") {
        anyhow::bail!(
            "replica mode requires shared state storage: set IRONFLOW_STORE to 'postgres' or 'redis' (got '{state}')"
        );
    }
    if !matches!(events, "postgres" | "redis") {
        anyhow::bail!(
            "replica mode requires shared durable events: set IRONFLOW_EVENT_STORE to 'postgres' or 'redis' (got '{events}')"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn process_local_backends_are_rejected() {
        assert!(super::validate_backends("json", "memory").is_err());
        assert!(super::validate_backends("sqlite", "postgres").is_err());
        assert!(super::validate_backends("postgres", "sqlite").is_err());
        super::validate_backends("postgres", "postgres").unwrap();
        super::validate_backends("redis", "redis").unwrap();
    }
}
