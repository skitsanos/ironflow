use anyhow::Result;
use mlua::prelude::*;

use crate::lua::analysis::HandlerDiagnostics;
use crate::nodes::NodeRegistry;

use super::handlers::serialize_handler;

pub(super) fn register_node_factories(
    lua: &Lua,
    registry: &NodeRegistry,
    diagnostics: Option<HandlerDiagnostics>,
) -> Result<()> {
    let nodes_table = lua.create_table()?;
    for (node_type, _description) in registry.list() {
        let node_type = node_type.to_string();
        let factory_node_type = node_type.clone();
        let diagnostics = diagnostics.clone();
        let factory = lua.create_function(move |lua, config: Option<LuaTable>| {
            let table = config.unwrap_or(lua.create_table()?);
            table.set("_node_type", factory_node_type.clone())?;

            if factory_node_type == "code"
                && let Ok(LuaValue::Function(function)) = table.get::<LuaValue>("source")
            {
                let bytecode = serialize_handler(&function, diagnostics.as_ref())?;
                table.set("bytecode_b64", bytecode)?;
                table.set("source", LuaValue::Nil)?;
            }

            if factory_node_type == "foreach"
                && let Ok(LuaValue::Function(function)) = table.get::<LuaValue>("transform")
            {
                let bytecode = serialize_handler(&function, diagnostics.as_ref())?;
                table.set("transform_bytecode_b64", bytecode)?;
                table.set("transform", LuaValue::Nil)?;
            }

            Ok(table)
        })?;
        nodes_table.set(node_type, factory)?;
    }
    lua.globals().set("nodes", nodes_table)?;
    Ok(())
}
