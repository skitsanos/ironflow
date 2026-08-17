use std::path::PathBuf;

use anyhow::Result;
use serde_json::Value;

use crate::engine::types::Context;
use crate::util::node_config::{config_f64_strict, config_usize_strict};

const DEFAULT_MAX_REPEAT_ITERATIONS: usize = 128;
const ABSOLUTE_MAX_REPEAT_ITERATIONS: usize = 1024;
const DEFAULT_MAX_DELAY_SECONDS: f64 = 60.0;
const ABSOLUTE_MAX_DELAY_SECONDS: f64 = 3600.0;

pub(super) struct RepeatConfig {
    pub(super) flow_file: String,
    pub(super) output_key: String,
    pub(super) state_key: String,
    pub(super) next_state_key: String,
    pub(super) until_key: String,
    pub(super) iteration_key: String,
    pub(super) max_iterations: usize,
    pub(super) delay_seconds: f64,
    pub(super) backoff_factor: f64,
    pub(super) max_delay_seconds: f64,
}

impl RepeatConfig {
    pub(super) fn resolve(config: &Value, ctx: &Context) -> Result<Self> {
        let flow_file = required_string(config, "flow")?;
        let output_key = optional_string(config, "output_key", "repeat_result")?;
        let state_key = optional_string(config, "state_key", "repeat_state")?;
        let next_state_key = optional_string(config, "next_state_key", "repeat_next_state")?;
        let until_key = optional_string(config, "until_key", "repeat_done")?;
        let iteration_key = optional_string(config, "iteration_key", "repeat_iteration")?;
        ensure_distinct_keys(&[
            ("state_key", state_key.as_str()),
            ("next_state_key", next_state_key.as_str()),
            ("until_key", until_key.as_str()),
            ("iteration_key", iteration_key.as_str()),
        ])?;

        let max_iterations = config_usize_strict(config, "max_iterations", ctx)?
            .ok_or_else(|| anyhow::anyhow!("repeat_subworkflow requires 'max_iterations'"))?;
        let process_cap = max_repeat_iterations()?;
        if max_iterations == 0 {
            anyhow::bail!("repeat_subworkflow: 'max_iterations' must be greater than 0");
        }
        if max_iterations > process_cap {
            anyhow::bail!(
                "repeat_subworkflow: max_iterations {} exceeds process limit of {} (IRONFLOW_MAX_REPEAT_ITERATIONS)",
                max_iterations,
                process_cap
            );
        }

        let delay_seconds = config_f64_strict(config, "delay_seconds", ctx)?.unwrap_or(0.0);
        let backoff_factor = config_f64_strict(config, "backoff_factor", ctx)?.unwrap_or(1.0);
        let max_delay_seconds = config_f64_strict(config, "max_delay_seconds", ctx)?
            .unwrap_or_else(|| delay_seconds.max(DEFAULT_MAX_DELAY_SECONDS));
        validate_delays(delay_seconds, backoff_factor, max_delay_seconds)?;

        validate_input(config, &next_state_key, &until_key, &iteration_key)?;

        Ok(Self {
            flow_file,
            output_key,
            state_key,
            next_state_key,
            until_key,
            iteration_key,
            max_iterations,
            delay_seconds,
            backoff_factor,
            max_delay_seconds,
        })
    }

    pub(super) fn initial_context(&self, config: &Value, parent: &Context) -> Result<Context> {
        let mut context = match config.get("input") {
            Some(Value::Object(input)) => input
                .iter()
                .map(|(child_key, source)| {
                    let value = source
                        .as_str()
                        .and_then(|parent_key| parent.get(parent_key))
                        .cloned()
                        .unwrap_or_else(|| source.clone());
                    (child_key.clone(), value)
                })
                .collect(),
            Some(_) => anyhow::bail!("repeat_subworkflow: 'input' must be an object"),
            None => parent.clone(),
        };
        context.remove(&self.next_state_key);
        context.remove(&self.until_key);
        context.remove(&self.iteration_key);
        Ok(context)
    }

    pub(super) fn unresolved_flow_path(&self, parent: &Context) -> Result<PathBuf> {
        let configured = PathBuf::from(&self.flow_file);
        if configured.is_absolute() {
            return Ok(configured);
        }
        let flow_dir = parent
            .get("_flow_dir")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "repeat_subworkflow: cannot resolve relative path '{}' — _flow_dir not set",
                    self.flow_file
                )
            })?;
        Ok(PathBuf::from(flow_dir).join(&self.flow_file))
    }
}

fn required_string(config: &Value, key: &str) -> Result<String> {
    let value = config
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("repeat_subworkflow requires non-empty '{key}'"))?;
    Ok(value.to_string())
}

fn optional_string(config: &Value, key: &str, default: &str) -> Result<String> {
    match config.get(key) {
        None => Ok(default.to_string()),
        Some(Value::String(value)) if !value.is_empty() => Ok(value.clone()),
        Some(_) => anyhow::bail!("repeat_subworkflow: '{key}' must be a non-empty string"),
    }
}

fn ensure_distinct_keys(keys: &[(&str, &str)]) -> Result<()> {
    for (index, (left_name, left_value)) in keys.iter().enumerate() {
        for (right_name, right_value) in &keys[index + 1..] {
            if left_value == right_value {
                anyhow::bail!(
                    "repeat_subworkflow: '{left_name}' and '{right_name}' must be distinct"
                );
            }
        }
    }
    Ok(())
}

fn validate_input(config: &Value, next_state: &str, until: &str, iteration: &str) -> Result<()> {
    let Some(input) = config.get("input") else {
        return Ok(());
    };
    let input = input
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("repeat_subworkflow: 'input' must be an object"))?;
    for reserved in [next_state, until, iteration] {
        if input.contains_key(reserved) {
            anyhow::bail!("repeat_subworkflow: input cannot set reserved child key '{reserved}'");
        }
    }
    Ok(())
}

fn validate_delays(delay: f64, backoff: f64, maximum: f64) -> Result<()> {
    if delay < 0.0 {
        anyhow::bail!("repeat_subworkflow: 'delay_seconds' must be non-negative");
    }
    if backoff < 1.0 {
        anyhow::bail!("repeat_subworkflow: 'backoff_factor' must be at least 1");
    }
    if maximum < delay {
        anyhow::bail!("repeat_subworkflow: 'max_delay_seconds' must be at least 'delay_seconds'");
    }
    if maximum > ABSOLUTE_MAX_DELAY_SECONDS {
        anyhow::bail!(
            "repeat_subworkflow: 'max_delay_seconds' cannot exceed {ABSOLUTE_MAX_DELAY_SECONDS}"
        );
    }
    Ok(())
}

fn max_repeat_iterations() -> Result<usize> {
    let Some(raw) = std::env::var_os("IRONFLOW_MAX_REPEAT_ITERATIONS") else {
        return Ok(DEFAULT_MAX_REPEAT_ITERATIONS);
    };
    let raw = raw.to_str().ok_or_else(|| {
        anyhow::anyhow!("IRONFLOW_MAX_REPEAT_ITERATIONS must contain valid UTF-8")
    })?;
    let value = raw.parse::<usize>().map_err(|_| {
        anyhow::anyhow!("IRONFLOW_MAX_REPEAT_ITERATIONS must be a positive whole number")
    })?;
    if value == 0 || value > ABSOLUTE_MAX_REPEAT_ITERATIONS {
        anyhow::bail!(
            "IRONFLOW_MAX_REPEAT_ITERATIONS must be between 1 and {ABSOLUTE_MAX_REPEAT_ITERATIONS}"
        );
    }
    Ok(value)
}
