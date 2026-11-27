use std::collections::{HashMap, HashSet};

use crate::engine::{recovery::ExecutionPlan, types::FlowDefinition};
use crate::lua::interpolate::context_reads;
use crate::nodes::http::output::{self, ResponseMode};

use super::report::Diagnostic;

#[derive(Clone, Copy)]
struct Producer<'a> {
    step: &'a str,
    phase: usize,
    mode: ResponseMode,
}

pub(super) fn warnings(
    flow: &FlowDefinition,
    plan: &ExecutionPlan,
    context_keys: &[String],
) -> Vec<Diagnostic> {
    let phases: HashMap<_, _> = plan
        .phases
        .iter()
        .enumerate()
        .flat_map(|(phase, names)| names.iter().map(move |name| (name.as_str(), phase)))
        .collect();
    let inputs: HashSet<&str> = context_keys.iter().map(String::as_str).collect();
    let mut prefixes = HashMap::new();
    let mut writes = HashMap::<String, Producer<'_>>::new();
    let mut templates = HashMap::<&str, usize>::new();

    for step in &flow.steps {
        let phase = phases[step.name.as_str()];
        if step.node_type == "template_render"
            && let Some(key) = step
                .config
                .get("output_key")
                .and_then(serde_json::Value::as_str)
        {
            templates
                .entry(key)
                .and_modify(|earliest| *earliest = (*earliest).min(phase))
                .or_insert(phase);
        }
        if step.node_type != "http_get" {
            continue;
        }
        let Ok(mode) = ResponseMode::parse(&step.config) else {
            continue;
        };
        let producer = Producer {
            step: &step.name,
            phase,
            mode,
        };
        let prefix = output::prefix(&step.config);
        // Don't suggest a replacement when several requests share the prefix.
        prefixes
            .entry(prefix)
            .and_modify(|value| *value = None)
            .or_insert(Some(producer));
        for field in mode.fields() {
            let earliest = writes.entry(field.key(prefix)).or_insert(producer);
            if phase < earliest.phase {
                *earliest = producer;
            }
        }
    }

    let mut warnings = Vec::new();
    for step in &flow.steps {
        if step.node_type != "template_render" {
            continue;
        }
        let Some(template) = step
            .config
            .get("template")
            .and_then(serde_json::Value::as_str)
        else {
            continue;
        };
        let phase = phases[step.name.as_str()];
        let mut seen = HashSet::new();
        // Malformed paths are rejected by the loader before reaching here.
        let _ = context_reads(template, |expression, key, field| {
            if inputs.contains(key) || templates.get(key).is_some_and(|earliest| *earliest < phase)
            {
                return;
            }
            let finding = if let Some(producer) = writes.get(key) {
                (producer.phase >= phase).then(|| {
                    ("context_read_before_http_output", format!(
                        "${{{expression}}} cannot read the output of '{}' in this phase; depend on that step",
                        producer.step
                    ))
                })
            } else {
                mismatch(&prefixes, expression, key, field, phase)
            };
            if let Some((code, message)) = finding
                && seen.insert((code, message.clone()))
            {
                warnings.push(Diagnostic::warning(code, message, &step.name));
            }
        });
    }
    warnings
}

fn mismatch(
    prefixes: &HashMap<&str, Option<Producer<'_>>>,
    expression: &str,
    key: &str,
    field: Option<&str>,
    phase: usize,
) -> Option<(&'static str, String)> {
    let (prefix, code) = if matches!(field, Some("data" | "artifact")) && prefixes.contains_key(key)
    {
        (key, "http_output_key_mismatch")
    } else {
        (
            key.strip_suffix("_data")
                .or_else(|| key.strip_suffix("_artifact"))?,
            "http_output_mode_mismatch",
        )
    };
    let producer = prefixes.get(prefix)?.as_ref()?;
    let expected = producer.mode.body_field().key(prefix);
    let timing = if producer.phase >= phase {
        "; also depend on that step to read its output"
    } else {
        ""
    };
    Some((
        code,
        format!(
            "${{{expression}}} is not the response body output of '{}'; use context key '{}' to read that response{timing}",
            producer.step, expected
        ),
    ))
}
