//! Process-wide outbound destination policy (IF-212).
//!
//! `IRONFLOW_BLOCK_PRIVATE_NETWORK` forces the destination guard on for every
//! covered node; `IRONFLOW_PRIVATE_NETWORK_ALLOWLIST` names the internal
//! destinations the operator still permits. Both have a config-file key that
//! the CLI installs as a fallback. The environment is read on each use, as the
//! numeric limits are, so an embedding caller needs no start-up call and a
//! malformed value fails the node closed instead of turning the guard off.

use std::sync::{Arc, Mutex, RwLock};

use anyhow::{Result, bail};

use super::allow::AllowList;

pub(crate) const BLOCK_PRIVATE_NETWORK: &str = "IRONFLOW_BLOCK_PRIVATE_NETWORK";
pub(crate) const PRIVATE_NETWORK_ALLOWLIST: &str = "IRONFLOW_PRIVATE_NETWORK_ALLOWLIST";

/// The resolved policy for one node execution.
#[derive(Clone, Debug, Default)]
pub(crate) struct OutboundPolicy {
    pub(crate) block_private_network: bool,
    pub(crate) allow: Arc<AllowList>,
}

#[derive(Default)]
struct ConfigFallback {
    block_private_network: Option<bool>,
    allow: Option<Arc<AllowList>>,
}

static CONFIG: RwLock<ConfigFallback> = RwLock::new(ConfigFallback {
    block_private_network: None,
    allow: None,
});

/// The last parsed environment allowlist, keyed by its raw text.
static PARSED: Mutex<Option<(String, Arc<AllowList>)>> = Mutex::new(None);

/// Install the `ironflow.yaml` values as the fallback below the environment.
/// A malformed allowlist entry fails here, at start-up.
pub fn install_config(
    block_private_network: Option<bool>,
    allowlist: Option<&[String]>,
) -> Result<()> {
    let allow = allowlist
        .map(|entries| {
            AllowList::parse(entries.iter().map(String::as_str))
                .map(Arc::new)
                .map_err(|error| {
                    anyhow::anyhow!("config file field 'private_network_allowlist': {error}")
                })
        })
        .transpose()?;
    let mut config = CONFIG.write().unwrap_or_else(|poison| poison.into_inner());
    config.block_private_network = block_private_network;
    config.allow = allow;
    Ok(())
}

/// Fail on a malformed environment value before any work starts.
pub fn validate_environment() -> Result<()> {
    from_environment().map(|_| ())
}

/// What start-up reports: whether the guard is forced on and how many
/// allowlist entries apply. Never the entries themselves.
pub fn summary() -> Result<(bool, usize)> {
    let policy = from_environment()?;
    Ok((policy.block_private_network, policy.allow.len()))
}

/// Resolve the policy: environment, then config file, then off and empty.
pub(crate) fn current() -> Result<OutboundPolicy> {
    #[cfg(test)]
    if let Some(policy) = super::test_scope::policy() {
        return Ok(policy);
    }
    from_environment()
}

fn from_environment() -> Result<OutboundPolicy> {
    let (configured_block, configured_allow) = {
        let config = CONFIG.read().unwrap_or_else(|poison| poison.into_inner());
        (config.block_private_network, config.allow.clone())
    };
    let block_private_network = match environment(BLOCK_PRIVATE_NETWORK)? {
        Some(raw) => parse_bool(&raw)?,
        None => configured_block.unwrap_or(false),
    };
    let allow = match environment(PRIVATE_NETWORK_ALLOWLIST)? {
        Some(raw) => parse_environment_allowlist(&raw)?,
        None => configured_allow.unwrap_or_default(),
    };
    Ok(OutboundPolicy {
        block_private_network,
        allow,
    })
}

fn parse_bool(raw: &str) -> Result<bool> {
    match raw.trim() {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => bail!("{BLOCK_PRIVATE_NETWORK} must be either 'true' or 'false'"),
    }
}

fn parse_environment_allowlist(raw: &str) -> Result<Arc<AllowList>> {
    let mut parsed = PARSED.lock().unwrap_or_else(|poison| poison.into_inner());
    if let Some((cached, list)) = parsed.as_ref()
        && cached == raw
    {
        return Ok(list.clone());
    }
    if raw.trim().is_empty() {
        bail!(
            "environment variable {PRIVATE_NETWORK_ALLOWLIST} is set but empty. Give it a value, or remove it to use 'private_network_allowlist' from the config file or no allowlist."
        );
    }
    let list = AllowList::parse(raw.split(','))
        .map(Arc::new)
        .map_err(|error| anyhow::anyhow!("{PRIVATE_NETWORK_ALLOWLIST}: {error}"))?;
    *parsed = Some((raw.to_owned(), list.clone()));
    Ok(list)
}

fn environment(name: &str) -> Result<Option<String>> {
    match std::env::var(name) {
        Ok(value) => Ok(Some(value)),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(std::env::VarError::NotUnicode(_)) => bail!("{name} must contain valid UTF-8"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_switch_accepts_only_true_or_false() {
        assert!(parse_bool("true").unwrap());
        assert!(!parse_bool(" false ").unwrap());
        for raw in ["", " ", "1", "yes", "on", "TRUE", "flase"] {
            let error = parse_bool(raw).unwrap_err().to_string();
            assert_eq!(
                error,
                "IRONFLOW_BLOCK_PRIVATE_NETWORK must be either 'true' or 'false'"
            );
        }
    }

    #[test]
    fn a_blank_or_malformed_environment_allowlist_fails() {
        for raw in ["", "  ", ","] {
            let error = parse_environment_allowlist(raw).unwrap_err().to_string();
            assert!(error.contains(PRIVATE_NETWORK_ALLOWLIST), "{error}");
        }
        let error = parse_environment_allowlist("db.internal,10.0.0.5/8")
            .unwrap_err()
            .to_string();
        assert!(error.contains("allowlist entry 2"), "{error}");
        assert!(!error.contains("10.0.0.5"), "{error}");
        let list = parse_environment_allowlist(" db.internal:5432 , 10.1.0.0/16 ").unwrap();
        assert_eq!(list.len(), 2);
    }
}
