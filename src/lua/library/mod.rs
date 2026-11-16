//! Bounded replacements for the Lua standard-library calls that can do
//! unbounded work inside one C call, where the instruction hook, the Lua
//! time budget and the step deadline cannot reach (IF-213).
//!
//! - `string.find`, `string.match`, `string.gmatch` and `string.gsub` run on
//!   a Rust port of the reference matcher that counts its steps.
//! - `string.rep` no longer loops for a zero-length result.
//! - `table.move`, and `table.insert` / `table.remove` on a table with a
//!   metatable, run as Lua loops past a small native fast path.
//!
//! The replacements raise ordinary string errors at the caller's position,
//! as the reference functions do (`library.lua`).
//!
//! Every other library call does work proportional to data the Lua memory
//! cap already bounds.

mod args;
mod budget;
mod classes;
mod find;
mod gsub;
mod pattern;
#[cfg(test)]
mod tests;

use std::sync::Arc;

use mlua::prelude::*;

pub(crate) use budget::LibraryBudget;

const LIBRARY: &str = include_str!("library.lua");

/// Replace the unbounded library functions of a new Lua state.
pub(crate) fn install(lua: &Lua) -> LuaResult<()> {
    let rust = lua.create_table()?;
    rust.set("FAIL", args::fail_marker())?;
    rust.set("RAW", args::raw_marker())?;
    rust.set("find", lua.create_function(find::find)?)?;
    rust.set("match", lua.create_function(find::string_match)?)?;
    rust.set("gmatch", lua.create_function(find::gmatch)?)?;
    rust.set("gsub", lua.create_function(gsub::gsub)?)?;
    lua.load(LIBRARY)
        .set_name("=ironflow.library")
        .call::<()>(rust)
}

/// Attach the budgets that apply inside library calls to `lua`.
pub(crate) fn set_budget(lua: &Lua, budget: LibraryBudget) {
    lua.set_app_data(Arc::new(budget));
}
