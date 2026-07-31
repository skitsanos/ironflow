use std::str::FromStr;

use anyhow::{Result, bail};
use clap::ArgMatches;
use clap::parser::ValueSource;

use super::IronFlowConfig;

/// The origin of command-line values that also have a YAML fallback.
///
/// Keeping this separately from the parsed values prevents an explicit value
/// that happens to equal a built-in default from being mistaken for a default.
#[derive(Debug, Default)]
pub(super) struct CommandValueSources {
    pub store_dir: Option<ValueSource>,
    pub host: Option<ValueSource>,
    pub port: Option<ValueSource>,
    pub flows_dir: Option<ValueSource>,
    pub max_body: Option<ValueSource>,
}

impl CommandValueSources {
    pub fn from_matches(matches: &ArgMatches) -> Self {
        let Some((name, command)) = matches.subcommand() else {
            return Self::default();
        };

        match name {
            "run" | "list" | "inspect" => Self {
                store_dir: command.value_source("store_dir"),
                ..Self::default()
            },
            "serve" => Self {
                store_dir: command.value_source("store_dir"),
                host: command.value_source("host"),
                port: command.value_source("port"),
                flows_dir: command.value_source("flows_dir"),
                max_body: command.value_source("max_body"),
            },
            _ => Self::default(),
        }
    }
}

fn is_cli_or_environment(source: Option<ValueSource>) -> bool {
    matches!(
        source,
        Some(ValueSource::CommandLine | ValueSource::EnvVariable)
    )
}

/// Resolve a parsed CLI value against its YAML fallback.
///
/// Clap has already applied `CLI > environment > default`. YAML only replaces
/// a value that Clap identifies as a built-in default.
pub(super) fn with_config<T>(parsed: T, source: Option<ValueSource>, config: Option<T>) -> T {
    if is_cli_or_environment(source) {
        parsed
    } else {
        config.unwrap_or(parsed)
    }
}

/// Resolve an optional parsed CLI value against its YAML fallback.
pub(super) fn optional_with_config<T>(
    parsed: Option<T>,
    source: Option<ValueSource>,
    config: Option<T>,
) -> Option<T> {
    if is_cli_or_environment(source) {
        parsed
    } else {
        config.or(parsed)
    }
}

pub(super) fn environment_string(name: &str) -> Result<Option<String>> {
    match std::env::var(name) {
        Ok(value) => Ok(Some(value)),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(std::env::VarError::NotUnicode(_)) => {
            bail!("{name} must contain valid UTF-8")
        }
    }
}

pub(super) fn environment_value<T>(name: &str, expected: &str) -> Result<Option<T>>
where
    T: FromStr,
{
    environment_string(name)?
        .map(|value| {
            value
                .parse()
                .map_err(|_| anyhow::anyhow!("{name} must be {expected}"))
        })
        .transpose()
}

#[derive(Debug)]
pub(super) struct ServerConfig {
    pub max_concurrent_tasks: Option<usize>,
    pub api_key: Option<String>,
    pub allow_unauthenticated_api: bool,
    pub allow_adhoc_flows: bool,
    pub cors_origins: Option<Vec<String>>,
}

impl ServerConfig {
    pub fn resolve(config: &IronFlowConfig) -> Result<Self> {
        validate_run_deadline_environment()?;
        // Unlike ordinary byte ceilings, an invalid admission limit cannot
        // safely fall back: doing so changes a bounded server into an
        // unlimited one while appearing to honor the operator's setting.
        let max_concurrent_runs =
            environment_value::<usize>("IRONFLOW_MAX_CONCURRENT_RUNS", "a non-negative integer")?;
        crate::util::runtime_config::validate_semaphore_limit(
            "IRONFLOW_MAX_CONCURRENT_RUNS",
            max_concurrent_runs,
        )?;
        let _ = crate::util::runtime_config::max_concurrent_flow_loads()?;

        Ok(Self {
            max_concurrent_tasks: resolve_max_concurrent_tasks(config)?,
            api_key: environment_string("IRONFLOW_API_KEY")?.or_else(|| config.api_key.clone()),
            allow_unauthenticated_api: environment_value(
                "IRONFLOW_ALLOW_UNAUTHENTICATED_API",
                "either 'true' or 'false'",
            )?
            .or(config.allow_unauthenticated_api)
            .unwrap_or(false),
            // Strict parsing, like every other environment toggle (IF-018): an
            // unrecognized value must fail loudly rather than silently resolve
            // to the permissive default, which would leave an operator who
            // wrote `off` or mistyped `flase` believing ad-hoc flows were
            // disabled when they were not (IF-056).
            allow_adhoc_flows: environment_value(
                "IRONFLOW_ALLOW_ADHOC_FLOWS",
                "either 'true' or 'false'",
            )?
            .or(config.allow_adhoc_flows)
            .unwrap_or(true),
            cors_origins: environment_string("IRONFLOW_CORS_ORIGINS")?
                .map(|value| {
                    value
                        .split(',')
                        .map(str::trim)
                        .filter(|origin| !origin.is_empty())
                        .map(str::to_string)
                        .collect()
                })
                .or_else(|| config.cors_origins.clone()),
        })
    }
}

/// Validate the process-wide deadline before any store is opened or run is
/// started. The coordinator reads the value lazily, but CLI entry points must
/// reject a typo instead of silently turning the deadline off.
pub(super) fn validate_run_deadline_environment() -> Result<()> {
    let _ = crate::util::runtime_config::run_deadline()?;
    Ok(())
}

pub(super) fn resolve_max_concurrent_tasks(config: &IronFlowConfig) -> Result<Option<usize>> {
    let value = environment_value("IRONFLOW_MAX_CONCURRENT_TASKS", "a non-negative integer")?
        .or(config.max_concurrent_tasks);
    crate::util::runtime_config::validate_semaphore_limit("IRONFLOW_MAX_CONCURRENT_TASKS", value)?;
    Ok(value)
}

#[cfg(test)]
mod tests {
    use clap::parser::ValueSource;

    use super::{optional_with_config, with_config};

    #[test]
    fn explicit_values_equal_to_defaults_still_win() {
        assert_eq!(
            with_config(3000_u16, Some(ValueSource::CommandLine), Some(9000)),
            3000
        );
        assert_eq!(
            with_config(3000_u16, Some(ValueSource::EnvVariable), Some(9000)),
            3000
        );
    }

    #[test]
    fn yaml_only_replaces_defaults_or_absent_options() {
        assert_eq!(
            with_config(3000_u16, Some(ValueSource::DefaultValue), Some(9000)),
            9000
        );
        assert_eq!(
            optional_with_config::<String>(None, None, Some("flows".to_string())),
            Some("flows".to_string())
        );
    }
}
