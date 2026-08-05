use anyhow::{Context, Result};
use mlua::prelude::*;

use crate::engine::types::FlowDefinition;
use crate::lua::analysis::{LuaDiagnostic, analyze_code_source};
use crate::util::limits::max_flow_source_bytes;

pub(super) fn validate_code_sources(
    lua: &Lua,
    flow: &FlowDefinition,
) -> Result<Vec<LuaDiagnostic>> {
    let max_bytes = max_flow_source_bytes();
    let mut total_bytes = 0_u64;
    let mut warnings = Vec::new();

    for step in &flow.steps {
        if step.node_type != "code" {
            continue;
        }
        if step
            .config
            .get("bytecode_b64")
            .and_then(|value| value.as_str())
            .is_some()
        {
            continue;
        }
        let Some(source) = step.config.get("source").and_then(|value| value.as_str()) else {
            continue;
        };

        total_bytes = add_source_bytes(total_bytes, source.len(), max_bytes)?;

        compile_code_source(lua, source, &step.name)?;
        warnings.extend(analyze_code_source(source, &step.name)?);
    }

    warnings.sort_by(|left, right| {
        left.step
            .cmp(&right.step)
            .then(left.line.cmp(&right.line))
            .then(left.column.cmp(&right.column))
    });
    Ok(warnings)
}

fn compile_code_source(lua: &Lua, source: &str, step: &str) -> Result<()> {
    let chunk_name = format!("<code:{step}>");
    if lua
        .load(source)
        .set_name(&chunk_name)
        .into_function()
        .is_ok()
    {
        return Ok(());
    }

    let expression_source = format!("return {source}");
    lua.load(&expression_source)
        .set_name(&chunk_name)
        .into_function()
        .map(|_| ())
        .map_err(|error| anyhow::anyhow!("Step '{step}' has invalid Lua code source: {error}"))
}

fn add_source_bytes(total: u64, source_bytes: usize, max_bytes: u64) -> Result<u64> {
    let total = total
        .checked_add(source_bytes as u64)
        .context("embedded Lua code source size overflow")?;
    if total > max_bytes {
        anyhow::bail!(
            "embedded Lua code sources exceed the {max_bytes}-byte flow-source limit \
             (raise IRONFLOW_MAX_FLOW_SOURCE_BYTES to allow it)"
        );
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aggregate_source_limit_is_inclusive() {
        assert_eq!(add_source_bytes(2, 2, 4).unwrap(), 4);
        let error = add_source_bytes(2, 3, 4).unwrap_err();
        assert!(error.to_string().contains("4-byte flow-source limit"));
    }

    #[test]
    fn expression_source_compiles_without_execution() {
        let lua = crate::lua::sandbox::new_sandboxed_lua().unwrap();
        compile_code_source(&lua, "1 + missing_value", "expression").unwrap();
    }
}
