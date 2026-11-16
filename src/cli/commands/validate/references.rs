//! The literal child-flow references of a flow's composition steps and
//! whether each one is certain to start and to matter (IF-220).

use std::collections::HashSet;

use serde_json::Value;

use crate::engine::types::{FlowDefinition, StepDefinition};

/// One literal child reference of a step.
pub(super) struct ChildRef {
    pub(super) step: String,
    pub(super) file: String,
    /// The child starts whenever the flow runs and its failure fails the
    /// flow: the step has no route, is no failure handler, depends on no such
    /// step and has no handler of its own, and the node always starts the
    /// child, waits for it and does not ignore its failure.
    pub(super) unconditional: bool,
}

/// Steps that may be skipped: routed steps, failure handlers and everything
/// that depends on one.
fn conditional_steps(flow: &FlowDefinition) -> HashSet<&str> {
    let mut conditional: HashSet<&str> = flow
        .steps
        .iter()
        .filter(|step| step.route.is_some())
        .map(|step| step.name.as_str())
        .chain(
            flow.steps
                .iter()
                .filter_map(|step| step.on_error.as_deref()),
        )
        .collect();
    loop {
        let before = conditional.len();
        for step in &flow.steps {
            if step
                .dependencies
                .iter()
                .any(|dependency| conditional.contains(dependency.as_str()))
            {
                conditional.insert(step.name.as_str());
            }
        }
        if conditional.len() == before {
            return conditional;
        }
    }
}

/// Whether a failed or refused child leaves the calling flow running: the
/// step has a failure handler, the node ignores child failures, or the child
/// is detached. `subworkflow` with `output_key` and no `on_error` ignores them.
fn tolerates_child_failure(step: &StepDefinition) -> bool {
    let config = &step.config;
    let on_error = config.get("on_error").and_then(Value::as_str);
    step.on_error.is_some()
        || on_error == Some("ignore")
        || config.get("wait").and_then(Value::as_bool) == Some(false)
        || (step.node_type == "subworkflow"
            && on_error.is_none()
            && config.get("output_key").is_some())
}

/// The literal child flows the composition steps of `flow` name. A path that
/// is only known at run time is not listed.
pub(super) fn child_refs(flow: &FlowDefinition) -> Vec<ChildRef> {
    let conditional = conditional_steps(flow);
    let mut refs = Vec::new();
    for step in &flow.steps {
        let always = !conditional.contains(step.name.as_str()) && !tolerates_child_failure(step);
        let literal = |entry: &Value| entry.get("flow").and_then(Value::as_str).map(str::to_owned);
        // (file, whether the node starts it on every execution)
        let named: Vec<(String, bool)> = match step.node_type.as_str() {
            "subworkflow" | "repeat_subworkflow" => literal(&step.config)
                .map(|file| (file, true))
                .into_iter()
                .collect(),
            "parallel_subworkflows" => match step.config.get("flows").and_then(Value::as_array) {
                Some(flows) => flows
                    .iter()
                    .filter_map(literal)
                    .map(|file| (file, true))
                    .collect(),
                // Dynamic mode fans out over a list that may be empty.
                None => literal(&step.config)
                    .map(|file| (file, false))
                    .into_iter()
                    .collect(),
            },
            // A tool handler runs only when the model calls that tool.
            "tool_dispatch" => step
                .config
                .get("tools")
                .and_then(Value::as_object)
                .map(|tools| {
                    tools
                        .values()
                        .filter_map(literal)
                        .map(|file| (file, false))
                        .collect()
                })
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        refs.extend(named.into_iter().map(|(file, starts)| ChildRef {
            step: step.name.clone(),
            file,
            unconditional: always && starts,
        }));
    }
    refs
}
