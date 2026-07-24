use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context as AnyhowContext, Result};
use serde_json::{Map, Value};

use crate::engine::executor::{ExecutionOverlay, WorkflowEngine};
use crate::engine::types::{Context, RunStatus};
use crate::lua::runtime::LuaRuntime;
use crate::nodes::NodeRegistry;
use crate::storage::null_store::NullStateStore;

use super::call_context::build_child_context;

pub(super) struct CallOutcome {
    pub(super) call_id: String,
    pub(super) entry: Value,
    pub(super) message: Value,
    pub(super) error: Option<String>,
}

pub(super) async fn dispatch_call(
    call: Value,
    tools: &Map<String, Value>,
    parent_ctx: &Context,
    child_registry: &Arc<NodeRegistry>,
    overlay: &ExecutionOverlay,
) -> Result<CallOutcome> {
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
        return Ok(unsupported_outcome(call, call_id, name));
    };

    let flow_file = mapping
        .get("flow")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("tool_dispatch: tool '{}' mapping requires 'flow'", name))?;
    let flow_path = resolve_flow_path(flow_file, parent_ctx)?;
    let mut child_ctx = build_child_context(mapping, parent_ctx, &call);
    if let Some(parent) = PathBuf::from(&flow_path).parent() {
        child_ctx.insert(
            "_flow_dir".to_string(),
            Value::String(parent.to_string_lossy().to_string()),
        );
    }
    overlay.strip_from_context(&mut child_ctx);

    let flow = LuaRuntime::load_flow_async(&flow_path, child_registry).await?;
    let flow_name = flow.name.clone();
    let store: Arc<dyn crate::storage::StateStore> = Arc::new(NullStateStore::new());
    let engine = WorkflowEngine::new(child_registry.clone(), store.clone(), None);
    let run_id = engine
        .start_with_execution_overlay(&flow, child_ctx, overlay.clone())
        .await?
        .wait_cancel_on_drop()
        .await?;
    let run_info = store.get_run_info(&run_id).await?;
    let succeeded = matches!(run_info.status, RunStatus::Success);
    let result = result_from_context(&run_info.ctx);
    let content = result_content(&result);
    let error = (!succeeded).then(|| {
        format!(
            "tool_dispatch: tool '{}' subworkflow finished with status: {}",
            name, run_info.status
        )
    });

    let mut entry = Map::new();
    entry.insert("success".to_string(), Value::Bool(succeeded));
    entry.insert("id".to_string(), Value::String(call_id.clone()));
    entry.insert("name".to_string(), Value::String(name));
    entry.insert(
        "arguments".to_string(),
        call.get("arguments").cloned().unwrap_or(Value::Null),
    );
    entry.insert("flow".to_string(), Value::String(flow_name));
    entry.insert("result".to_string(), result);
    entry.insert("content".to_string(), Value::String(content.clone()));
    if let Some(message) = &error {
        entry.insert("error".to_string(), Value::String(message.clone()));
    }
    Ok(CallOutcome {
        call_id,
        entry: Value::Object(entry),
        message: tool_message(&call, content),
        error,
    })
}

fn unsupported_outcome(call: Value, call_id: String, name: String) -> CallOutcome {
    let error = format!("tool_dispatch: unsupported tool '{}'", name);
    let result = serde_json::json!({ "error": error });
    let entry = serde_json::json!({
        "success": false,
        "id": call_id,
        "name": name,
        "arguments": call.get("arguments").cloned().unwrap_or(Value::Null),
        "error": result.get("error").cloned().unwrap_or(Value::Null),
        "result": result
    });
    CallOutcome {
        message: tool_message(&call, result_content(&result)),
        call_id,
        entry,
        error: Some(error),
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

fn result_from_context(ctx: &Context) -> Value {
    ctx.get("tool_result_value")
        .cloned()
        .or_else(|| ctx.get("tool_result_text").cloned())
        .unwrap_or_else(|| {
            Value::Object(
                ctx.iter()
                    .filter(|(key, _)| !key.starts_with('_'))
                    .map(|(key, value)| (key.clone(), value.clone()))
                    .collect(),
            )
        })
}

fn result_content(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

fn tool_message(call: &Value, content: String) -> Value {
    serde_json::json!({
        "role": "tool",
        "tool_call_id": call.get("id").and_then(Value::as_str).unwrap_or(""),
        "content": content
    })
}
