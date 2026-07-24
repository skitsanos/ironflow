mod call_context;
mod runner;

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{Map, Value};

use crate::engine::executor::ExecutionOverlay;
use crate::engine::types::{Context, NodeOutput};
use crate::nodes::ai::llm_response::normalize_tool_calls;
use crate::nodes::{Node, NodeRegistry};
use crate::util::node_config::config_usize_strict;

use super::parallel_subworkflows::ParallelSubworkflowsNode;
use super::subworkflow::SubworkflowNode;
use runner::{CallOutcome, dispatch_call};

const DEFAULT_MAX_TOOL_CALLS: usize = 32;

pub struct ToolDispatchNode {
    /// Registry containing all non-subworkflow composition nodes.
    pub base_registry: Arc<NodeRegistry>,
}

impl ToolDispatchNode {
    fn child_registry(&self) -> Arc<NodeRegistry> {
        let mut child = self.base_registry.snapshot();
        child.register(Arc::new(SubworkflowNode {
            base_registry: self.base_registry.clone(),
        }));
        child.register(Arc::new(ParallelSubworkflowsNode {
            base_registry: self.base_registry.clone(),
        }));
        child.register(Arc::new(ToolDispatchNode {
            base_registry: self.base_registry.clone(),
        }));
        Arc::new(child)
    }
}

#[async_trait]
impl Node for ToolDispatchNode {
    fn node_type(&self) -> &str {
        "tool_dispatch"
    }

    fn description(&self) -> &str {
        "Dispatch llm tool calls to mapped subworkflow handlers"
    }

    async fn execute(&self, config: &Value, ctx: &Context) -> Result<NodeOutput> {
        let source_key = config
            .get("source_key")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow::anyhow!("tool_dispatch requires 'source_key'"))?;
        let output_key = config
            .get("output_key")
            .and_then(Value::as_str)
            .unwrap_or("tool_results");
        let tools = config
            .get("tools")
            .and_then(Value::as_object)
            .ok_or_else(|| anyhow::anyhow!("tool_dispatch requires 'tools' object"))?;
        let fail_fast = parse_fail_fast(config)?;
        let max_calls =
            config_usize_strict(config, "max_calls", ctx)?.unwrap_or(DEFAULT_MAX_TOOL_CALLS);
        if max_calls == 0 {
            anyhow::bail!("tool_dispatch: 'max_calls' must be greater than 0");
        }

        let source = ctx.get(source_key).ok_or_else(|| {
            anyhow::anyhow!(
                "tool_dispatch: source_key '{}' not found in context",
                source_key
            )
        })?;
        let calls = normalized_calls(source)?;
        if calls.len() > max_calls {
            anyhow::bail!(
                "tool_dispatch: {} tool calls exceeds max_calls limit of {}",
                calls.len(),
                max_calls
            );
        }

        let child_registry = self.child_registry();
        let overlay = ExecutionOverlay::current();
        let mut results = DispatchResults::new(calls.len());
        for call in calls {
            let outcome = dispatch_call(call, tools, ctx, &child_registry, &overlay).await?;
            if fail_fast && let Some(error) = &outcome.error {
                return Err(anyhow::anyhow!(error.clone()));
            }
            results.push(outcome);
        }
        Ok(results.into_output(output_key))
    }
}

fn parse_fail_fast(config: &Value) -> Result<bool> {
    match config
        .get("on_error")
        .and_then(Value::as_str)
        .unwrap_or("fail_fast")
    {
        "fail_fast" => Ok(true),
        "ignore" => Ok(false),
        other => anyhow::bail!(
            "tool_dispatch: invalid on_error '{}'; expected 'fail_fast' or 'ignore'",
            other
        ),
    }
}

fn normalized_calls(value: &Value) -> Result<Vec<Value>> {
    let calls = value
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("tool_dispatch: source_key value must be an array"))?;
    let already_normalized = calls.iter().all(|call| {
        call.get("name").and_then(Value::as_str).is_some()
            && call.get("arguments").is_some()
            && call.get("raw_arguments").is_some()
    });
    Ok(if already_normalized {
        calls.clone()
    } else {
        normalize_tool_calls(calls)
    })
}

struct DispatchResults {
    entries: Vec<Value>,
    messages: Vec<Value>,
    by_id: Map<String, Value>,
    error_count: usize,
}

impl DispatchResults {
    fn new(capacity: usize) -> Self {
        Self {
            entries: Vec::with_capacity(capacity),
            messages: Vec::with_capacity(capacity),
            by_id: Map::new(),
            error_count: 0,
        }
    }

    fn push(&mut self, outcome: CallOutcome) {
        self.error_count += usize::from(outcome.error.is_some());
        self.messages.push(outcome.message);
        if !outcome.call_id.is_empty() {
            self.by_id.insert(outcome.call_id, outcome.entry.clone());
        }
        self.entries.push(outcome.entry);
    }

    fn into_output(self, output_key: &str) -> NodeOutput {
        let count = self.messages.len();
        let mut output = NodeOutput::new();
        output.insert(output_key.to_string(), Value::Array(self.entries));
        output.insert(format!("{}_count", output_key), Value::from(count));
        output.insert(
            format!("{}_errors", output_key),
            Value::from(self.error_count),
        );
        output.insert(
            format!("{}_all_succeeded", output_key),
            Value::Bool(self.error_count == 0),
        );
        output.insert(
            format!("{}_messages", output_key),
            Value::Array(self.messages),
        );
        output.insert(format!("{}_by_id", output_key), Value::Object(self.by_id));
        output
    }
}
