//! Per-step option validation: interpolation templates, retry and timeout
//! bounds, and route placement.

use crate::engine::step_timing::{deadline_after, first_unschedulable_retry};
use crate::engine::types::{FlowDefinition, StepDefinition};

/// Upper bound on a step's retry count. A count near `u32::MAX` combined with
/// `backoff(0)` produces a near-infinite retry storm (each attempt writes task
/// state and publishes events), so counts beyond this are rejected at
/// validation. Real workflows never need more than a handful of retries.
const MAX_RETRY_COUNT: u32 = 100;

pub(super) fn validate_step_options(flow: &FlowDefinition) -> Vec<String> {
    let mut errors = Vec::new();

    for step in &flow.steps {
        for (path, error) in crate::lua::interpolate::validate_value(&step.config) {
            errors.push(format!("Step '{}' {path}: {error}", step.name));
        }
        validate_retry(step, &mut errors);
        validate_timeout(step, &mut errors);
        // A route is matched against `_route_<dep>` keys emitted by the step's
        // dependencies, so a routed step with no dependencies can never match
        // and is silently always skipped (IF-052).
        if step.route.is_some() && step.dependencies.is_empty() {
            errors.push(format!(
                "Step '{}' declares a route but has no dependencies; a routed step must depend on the step that emits the route decision",
                step.name
            ));
        }
    }

    errors
}

fn validate_retry(step: &StepDefinition, errors: &mut Vec<String>) {
    let count_in_range = step.retry.max_retries <= MAX_RETRY_COUNT;
    if !count_in_range {
        errors.push(format!(
            "Step '{}' retry count is too large (max {MAX_RETRY_COUNT})",
            step.name
        ));
    }
    if !step.retry.backoff_s.is_finite() || step.retry.backoff_s < 0.0 {
        errors.push(format!(
            "Step '{}' retry backoff must be a finite, non-negative number of seconds",
            step.name
        ));
    } else if count_in_range
        && let Some(retry) = first_unschedulable_retry(step.retry.backoff_s, step.retry.max_retries)
    {
        // The executor would otherwise discover this between attempts, with
        // the task already recorded as running (IF-203).
        errors.push(format!(
            "Step '{}' retry backoff is too large to schedule: the wait before retry {retry} is not representable as a duration",
            step.name
        ));
    }
}

fn validate_timeout(step: &StepDefinition, errors: &mut Vec<String>) {
    let Some(timeout) = step.timeout_s else {
        return;
    };
    if !timeout.is_finite() || timeout <= 0.0 {
        errors.push(format!(
            "Step '{}' timeout must be a finite number greater than zero",
            step.name
        ));
    } else if deadline_after(timeout).is_err() {
        errors.push(format!(
            "Step '{}' timeout is too large to schedule",
            step.name
        ));
    }
}
