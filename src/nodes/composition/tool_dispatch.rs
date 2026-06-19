use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context as AnyhowContext, Result};
use async_trait::async_trait;
use serde_json::{Map, Value};

use crate::engine::executor::WorkflowEngine;
use crate::engine::types::{Context, NodeOutput, RunStatus};
use crate::lua::runtime::LuaRuntime;
use crate::nodes::ai::llm_response::normalize_tool_calls;
use crate::nodes::{Node, NodeRegistry};
use crate::storage::null_store::NullStateStore;

use super::parallel_subworkflows::ParallelSubworkflowsNode;
use super::subworkflow::SubworkflowNode;

const DEFAULT_MAX_TOOL_CALLS: usize = 32;

pub struct ToolDispatchNode {
    /// Registry containing all non-subworkflow composition nodes.
    /// At execution time, we add composition nodes back so tool handlers can
    /// run full child workflows.
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

fn normalized_calls_from_value(value: &Value) -> Result<Vec<Value>> {
    let calls = value
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("tool_dispatch: source_key value must be an array"))?;

    let already_normalized = calls.iter().all(|call| {
        call.get("name").and_then(Value::as_str).is_some()
            && call.get("arguments").is_some()
            && call.get("raw_arguments").is_some()
    });

    if already_normalized {
        Ok(calls.clone())
    } else {
        Ok(normalize_tool_calls(calls))
    }
}

fn resolve_path<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    if path.is_empty() {
        return Some(value);
    }

    let mut current = value;
    for part in path.split('.') {
        if part.is_empty() {
            return None;
        }

        if let Ok(idx) = part.parse::<usize>() {
            current = current.as_array()?.get(idx)?;
        } else {
            current = current.get(part)?;
        }
    }

    Some(current)
}

fn resolve_input_value(spec: &Value, parent_ctx: &Context, call: &Value) -> Value {
    match spec {
        Value::String(s) => {
            if let Some(path) = s.strip_prefix("arguments.") {
                return resolve_path(call.get("arguments").unwrap_or(&Value::Null), path)
                    .cloned()
                    .unwrap_or(Value::Null);
            }
            if s == "arguments" {
                return call.get("arguments").cloned().unwrap_or(Value::Null);
            }
            if let Some(path) = s.strip_prefix("call.") {
                return resolve_path(call, path).cloned().unwrap_or(Value::Null);
            }
            if s == "call" {
                return call.clone();
            }
            if let Some(path) = s.strip_prefix("ctx.") {
                let mut parts = path.splitn(2, '.');
                let first = parts.next().unwrap_or_default();
                let Some(root) = parent_ctx.get(first) else {
                    return Value::Null;
                };
                return parts
                    .next()
                    .and_then(|rest| resolve_path(root, rest))
                    .cloned()
                    .unwrap_or_else(|| root.clone());
            }
            if s == "tool_name" {
                return call.get("name").cloned().unwrap_or(Value::Null);
            }
            if s == "tool_call_id" {
                return call.get("id").cloned().unwrap_or(Value::Null);
            }
            if let Some(value) = parent_ctx.get(s) {
                return value.clone();
            }

            Value::String(s.clone())
        }
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|item| resolve_input_value(item, parent_ctx, call))
                .collect(),
        ),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, value)| (key.clone(), resolve_input_value(value, parent_ctx, call)))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn build_child_context(mapping: &Value, parent_ctx: &Context, call: &Value) -> Context {
    let mut child_ctx = Context::new();
    child_ctx.insert("tool_call".to_string(), call.clone());
    child_ctx.insert(
        "tool_name".to_string(),
        call.get("name").cloned().unwrap_or(Value::Null),
    );
    child_ctx.insert(
        "tool_arguments".to_string(),
        call.get("arguments").cloned().unwrap_or(Value::Null),
    );
    child_ctx.insert(
        "tool_call_id".to_string(),
        call.get("id").cloned().unwrap_or(Value::Null),
    );
    child_ctx.insert(
        "tool_call_index".to_string(),
        call.get("index").cloned().unwrap_or(Value::Null),
    );

    if let Some(input) = mapping.get("input").and_then(Value::as_object) {
        for (key, spec) in input {
            child_ctx.insert(key.clone(), resolve_input_value(spec, parent_ctx, call));
        }
    }

    child_ctx
}

fn filtered_context(ctx: &Context) -> Map<String, Value> {
    ctx.iter()
        .filter(|(key, _)| !key.starts_with('_'))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
}

fn result_value_from_context(ctx: &Context) -> Value {
    ctx.get("tool_result_value")
        .cloned()
        .or_else(|| ctx.get("tool_result_text").cloned())
        .unwrap_or_else(|| Value::Object(filtered_context(ctx)))
}

fn result_content(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn resolve_flow_path(flow_file: &str, ctx: &Context) -> Result<String> {
    let flow_path = if PathBuf::from(flow_file).is_absolute() {
        PathBuf::from(flow_file)
    } else {
        let flow_dir = ctx
            .get("_flow_dir")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "tool_dispatch: cannot resolve relative path '{}' - _flow_dir not set",
                    flow_file
                )
            })?;
        PathBuf::from(flow_dir).join(flow_file)
    };

    Ok(flow_path
        .canonicalize()
        .with_context(|| format!("tool_dispatch: cannot find '{}'", flow_path.display()))?
        .to_string_lossy()
        .to_string())
}

fn tool_message(call: &Value, content: String) -> Value {
    serde_json::json!({
        "role": "tool",
        "tool_call_id": call.get("id").and_then(Value::as_str).unwrap_or(""),
        "content": content
    })
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
        let fail_fast = match config
            .get("on_error")
            .and_then(Value::as_str)
            .unwrap_or("fail_fast")
        {
            "fail_fast" => true,
            "ignore" => false,
            other => {
                return Err(anyhow::anyhow!(
                    "tool_dispatch: invalid on_error '{}'; expected 'fail_fast' or 'ignore'",
                    other
                ));
            }
        };
        let max_calls = config
            .get("max_calls")
            .and_then(Value::as_u64)
            .map(|n| n as usize)
            .filter(|n| *n > 0)
            .unwrap_or(DEFAULT_MAX_TOOL_CALLS);

        let source = ctx.get(source_key).ok_or_else(|| {
            anyhow::anyhow!(
                "tool_dispatch: source_key '{}' not found in context",
                source_key
            )
        })?;
        let calls = normalized_calls_from_value(source)?;
        if calls.len() > max_calls {
            return Err(anyhow::anyhow!(
                "tool_dispatch: {} tool calls exceeds max_calls limit of {}",
                calls.len(),
                max_calls
            ));
        }

        let child_registry = self.child_registry();
        let mut results = Vec::with_capacity(calls.len());
        let mut messages = Vec::with_capacity(calls.len());
        let mut by_id = Map::new();
        let mut errors = Vec::new();

        for call in calls {
            let name = call
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string();
            let call_id = call
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let Some(mapping) = tools.get(&name) else {
                let message = format!("tool_dispatch: unsupported tool '{}'", name);
                if fail_fast {
                    return Err(anyhow::anyhow!(message));
                }

                let result_value = serde_json::json!({ "error": message });
                let entry = serde_json::json!({
                    "success": false,
                    "id": call_id,
                    "name": name,
                    "arguments": call.get("arguments").cloned().unwrap_or(Value::Null),
                    "error": result_value.get("error").cloned().unwrap_or(Value::Null),
                    "result": result_value
                });
                errors.push(message.clone());
                messages.push(tool_message(&call, result_content(&result_value)));
                if !call_id.is_empty() {
                    by_id.insert(call_id, entry.clone());
                }
                results.push(entry);
                continue;
            };

            let flow_file = mapping.get("flow").and_then(Value::as_str).ok_or_else(|| {
                anyhow::anyhow!("tool_dispatch: tool '{}' mapping requires 'flow'", name)
            })?;
            let flow_path = resolve_flow_path(flow_file, ctx)?;
            let mut child_ctx = build_child_context(mapping, ctx, &call);
            if let Some(parent) = PathBuf::from(&flow_path).parent() {
                child_ctx.insert(
                    "_flow_dir".to_string(),
                    Value::String(parent.to_string_lossy().to_string()),
                );
            }

            let flow = LuaRuntime::load_flow(&flow_path, &child_registry)?;
            let flow_name = flow.name.clone();
            let store: Arc<dyn crate::storage::StateStore> = Arc::new(NullStateStore::new());
            let engine = WorkflowEngine::new(child_registry.clone(), store.clone(), None);
            let run_id = engine.execute(&flow, child_ctx).await?;
            let run_info = store.get_run_info(&run_id).await?;
            let succeeded = matches!(run_info.status, RunStatus::Success);
            let result_value = result_value_from_context(&run_info.ctx);
            let content = result_content(&result_value);

            let mut entry = Map::new();
            entry.insert("success".to_string(), Value::Bool(succeeded));
            entry.insert("id".to_string(), Value::String(call_id.clone()));
            entry.insert("name".to_string(), Value::String(name.clone()));
            entry.insert(
                "arguments".to_string(),
                call.get("arguments").cloned().unwrap_or(Value::Null),
            );
            entry.insert("flow".to_string(), Value::String(flow_name));
            entry.insert("result".to_string(), result_value);
            entry.insert("content".to_string(), Value::String(content.clone()));

            if !succeeded {
                let message = format!(
                    "tool_dispatch: tool '{}' subworkflow finished with status: {}",
                    name, run_info.status
                );
                entry.insert("error".to_string(), Value::String(message.clone()));
                errors.push(message.clone());
                if fail_fast {
                    return Err(anyhow::anyhow!(message));
                }
            }

            let entry = Value::Object(entry);
            messages.push(tool_message(&call, content));
            if !call_id.is_empty() {
                by_id.insert(call_id, entry.clone());
            }
            results.push(entry);
        }

        let mut output = NodeOutput::new();
        output.insert(output_key.to_string(), Value::Array(results));
        output.insert(
            format!("{}_count", output_key),
            Value::Number(messages.len().into()),
        );
        output.insert(
            format!("{}_errors", output_key),
            Value::Number(errors.len().into()),
        );
        output.insert(
            format!("{}_all_succeeded", output_key),
            Value::Bool(errors.is_empty()),
        );
        output.insert(format!("{}_messages", output_key), Value::Array(messages));
        output.insert(format!("{}_by_id", output_key), Value::Object(by_id));

        Ok(output)
    }
}
