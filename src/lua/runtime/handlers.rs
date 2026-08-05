use mlua::prelude::*;

use crate::lua::analysis::HandlerDiagnostics;

pub(super) fn serialize_handler(
    function: &LuaFunction,
    diagnostics: Option<&HandlerDiagnostics>,
) -> LuaResult<String> {
    let info = function.info();
    let environment_upvalue = u8::from(function.environment().is_some());
    let captured_upvalues = info.num_upvalues.saturating_sub(environment_upvalue);
    if captured_upvalues > 0 {
        let line = info.line_defined.unwrap_or(0);
        return Err(LuaError::RuntimeError(format!(
            "Lua function handler at line {line} captures {captured_upvalues} outer local value(s), but captured upvalues cannot survive handler serialization; move constants inside the handler or pass values through ctx"
        )));
    }

    if let Some(diagnostics) = diagnostics {
        let line = info.line_defined.ok_or_else(|| {
            LuaError::RuntimeError("Serialized Lua handler has no source line".to_string())
        })?;
        let end_line = info.last_line_defined.unwrap_or(line);
        diagnostics
            .record_handler(line, end_line)
            .map_err(LuaError::external)?;
    }

    Ok(crate::lua::bytecode::sign(&function.dump(false)))
}
