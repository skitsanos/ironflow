//! Present-but-blank configuration values (IF-198).
//!
//! A variable that is set to an empty or whitespace-only string is a common
//! deployment artefact: a compose placeholder, or an orchestrator secret that
//! did not resolve. It is neither "unset" nor a usable value, so resolving it
//! either way would silently replace what the operator configured. These
//! helpers turn it into a startup error that names the source and never the
//! value.

use std::fmt;

use anyhow::{Result, bail};

use super::resolution::environment_string;

/// Where a configuration value came from, for diagnostics.
#[derive(Clone, Copy, Debug)]
pub(super) enum SettingSource {
    Environment(&'static str),
    ConfigFile(&'static str),
}

impl fmt::Display for SettingSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Environment(name) => write!(formatter, "environment variable {name}"),
            Self::ConfigFile(field) => write!(formatter, "config file field '{field}'"),
        }
    }
}

pub(super) fn is_blank(value: &str) -> bool {
    value.trim().is_empty()
}

/// Reject a present but blank value; pass an absent or usable one through.
pub(super) fn reject_blank(source: SettingSource, value: Option<String>) -> Result<Option<String>> {
    match value {
        Some(value) if is_blank(&value) => bail!(
            "{source} is set but empty. Give it a value, or remove it to use the lower-priority setting or the default."
        ),
        value => Ok(value),
    }
}

/// Read an environment variable whose blank value must not be mistaken for
/// either "unset" or a real setting.
pub(super) fn environment_non_blank(name: &'static str) -> Result<Option<String>> {
    reject_blank(SettingSource::Environment(name), environment_string(name)?)
}

/// Resolve the API key from the environment value and its YAML fallback.
///
/// The environment wins whenever the variable is present, as for every other
/// setting. A blank winner fails instead of meaning "no key", because a
/// loopback server would then start unauthenticated while a key sits in the
/// lower-priority source. Only the explicit unauthenticated opt-in makes a
/// blank key resolve to no key.
pub(super) fn resolve_api_key(
    environment: Option<String>,
    config: Option<String>,
    allow_unauthenticated_api: bool,
) -> Result<Option<String>> {
    let (source, value) = match environment {
        Some(value) => (SettingSource::Environment("IRONFLOW_API_KEY"), Some(value)),
        None => (SettingSource::ConfigFile("api_key"), config),
    };
    match value {
        Some(value) if is_blank(&value) => {
            if allow_unauthenticated_api {
                return Ok(None);
            }
            bail!(
                "{source} is set but empty, so the API key would not be enforced. Give it a value, remove it, or set IRONFLOW_ALLOW_UNAUTHENTICATED_API=true to serve without authentication on purpose."
            )
        }
        value => Ok(value),
    }
}

/// Parse the comma-separated origin list of `IRONFLOW_CORS_ORIGINS`. A value
/// with no origin in it would replace the configured list with none.
pub(super) fn cors_origins_from_environment(value: Option<String>) -> Result<Option<Vec<String>>> {
    let Some(value) = value else {
        return Ok(None);
    };
    let origins: Vec<String> = value
        .split(',')
        .map(str::trim)
        .filter(|origin| !origin.is_empty())
        .map(str::to_string)
        .collect();
    if origins.is_empty() {
        bail!(
            "environment variable IRONFLOW_CORS_ORIGINS is set but empty: it contains no origin. Give it a value, or remove it to use 'cors_origins' from the config file or the deny-by-default policy."
        );
    }
    Ok(Some(origins))
}

#[cfg(test)]
mod tests {
    use super::{SettingSource, cors_origins_from_environment, reject_blank, resolve_api_key};

    const BLANKS: [&str; 4] = ["", " ", "\t", " \r\n "];

    fn some(value: &str) -> Option<String> {
        Some(value.to_string())
    }

    #[test]
    fn blank_values_fail_and_name_only_the_source() {
        for blank in BLANKS {
            let error = reject_blank(SettingSource::Environment("REDIS_PREFIX"), some(blank))
                .unwrap_err()
                .to_string();
            assert!(error.contains("environment variable REDIS_PREFIX is set but empty"));
        }
        let source = SettingSource::Environment("REDIS_PREFIX");
        assert_eq!(reject_blank(source, None).unwrap(), None);
        assert_eq!(reject_blank(source, some("app:")).unwrap(), some("app:"));
    }

    #[test]
    fn blank_api_key_never_falls_through_to_no_key() {
        for blank in BLANKS {
            let error = resolve_api_key(some(blank), some("configured-key"), false)
                .unwrap_err()
                .to_string();
            assert!(error.contains("environment variable IRONFLOW_API_KEY"));
            assert!(!error.contains("configured-key"));

            let error = resolve_api_key(None, some(blank), false)
                .unwrap_err()
                .to_string();
            assert!(error.contains("config file field 'api_key'"));
        }
    }

    #[test]
    fn api_key_precedence_and_explicit_opt_in_are_unchanged() {
        assert_eq!(
            resolve_api_key(some("from-env"), some("from-yaml"), false).unwrap(),
            some("from-env")
        );
        assert_eq!(
            resolve_api_key(None, some("from-yaml"), false).unwrap(),
            some("from-yaml")
        );
        assert_eq!(resolve_api_key(None, None, false).unwrap(), None);
        assert_eq!(
            resolve_api_key(some(""), some("from-yaml"), true).unwrap(),
            None
        );
        assert_eq!(resolve_api_key(None, some(" "), true).unwrap(), None);
        // The opt-in does not discard a usable key.
        assert_eq!(
            resolve_api_key(some("from-env"), None, true).unwrap(),
            some("from-env")
        );
    }

    #[test]
    fn cors_variable_without_an_origin_fails() {
        for blank in ["", " ", ",", " , ,"] {
            let error = cors_origins_from_environment(some(blank))
                .unwrap_err()
                .to_string();
            assert!(error.contains("IRONFLOW_CORS_ORIGINS"));
        }
        assert_eq!(cors_origins_from_environment(None).unwrap(), None);
        assert_eq!(
            cors_origins_from_environment(some("https://a.example, ,https://b.example")).unwrap(),
            Some(vec![
                "https://a.example".to_string(),
                "https://b.example".to_string()
            ])
        );
    }
}
