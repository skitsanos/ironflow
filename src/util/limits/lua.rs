//! Lua VM execution limits: instruction/time/memory budgets enforced via a
//! debug hook, plus post-execution garbage collection.

use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{Duration, Instant};

use anyhow::Result;
use mlua::prelude::*;

use crate::util::execution::ExecutionControl;

use super::{
    lua_gc_after_execution, lua_hook_interval, max_lua_instructions, max_lua_memory_bytes,
    max_lua_seconds,
};

#[derive(Clone, Copy, Debug)]
pub struct LuaExecutionLimits {
    pub max_instructions: Option<u64>,
    pub max_seconds: Option<u64>,
    pub max_memory_bytes: Option<u64>,
    pub hook_interval: u32,
    pub gc_after_execution: bool,
}

impl LuaExecutionLimits {
    pub fn from_env() -> Self {
        Self {
            max_instructions: max_lua_instructions(),
            max_seconds: max_lua_seconds(),
            max_memory_bytes: max_lua_memory_bytes(),
            hook_interval: lua_hook_interval().min(u32::MAX as u64) as u32,
            gc_after_execution: lua_gc_after_execution(),
        }
    }
}

pub fn apply_lua_limits(lua: &Lua, limits: LuaExecutionLimits) -> Result<()> {
    apply_lua_limits_with_control(lua, limits, None)
}

/// Apply Lua resource limits plus a step deadline/cancellation hook.
pub fn apply_lua_limits_with_control(
    lua: &Lua,
    limits: LuaExecutionLimits,
    execution: Option<ExecutionControl>,
) -> Result<()> {
    lua.gc_restart();
    // mlua 0.12 replaced `gc_inc(pause, step_multiplier, step_size)` with an
    // explicit incremental GC mode.
    lua.gc_set_mode(LuaGcMode::Incremental(
        LuaGcIncParams::default()
            .pause(200)
            .step_multiplier(200)
            .step_size(13),
    ));

    if let Some(max_memory_bytes) = limits.max_memory_bytes {
        lua.set_memory_limit(max_memory_bytes as usize)?;
    }

    let hook_interval = limits.hook_interval.max(1);
    if limits.max_instructions.is_none() && limits.max_seconds.is_none() && execution.is_none() {
        return Ok(());
    }

    let remaining = limits
        .max_instructions
        .map(|max| Arc::new(AtomicI64::new(max.min(i64::MAX as u64) as i64)));
    let max_duration = limits.max_seconds.map(Duration::from_secs);
    let started = Instant::now();

    lua.set_hook(
        LuaHookTriggers::new().every_nth_instruction(hook_interval),
        move |_lua, _debug| {
            if let Some(ref execution) = execution {
                execution
                    .checkpoint()
                    .map_err(|error| LuaError::runtime(error.to_string()))?;
            }

            if let Some(ref remaining) = remaining
                && remaining.fetch_sub(hook_interval as i64, Ordering::Relaxed)
                    <= hook_interval as i64
            {
                return Err(LuaError::runtime(format!(
                    "Lua execution exceeded instruction budget of {}",
                    limits.max_instructions.unwrap_or_default()
                )));
            }

            if let Some(max_duration) = max_duration
                && started.elapsed() >= max_duration
            {
                return Err(LuaError::runtime(format!(
                    "Lua execution exceeded time budget of {}s",
                    max_duration.as_secs()
                )));
            }

            Ok(LuaVmState::Continue)
        },
    )?;

    Ok(())
}

pub fn collect_lua_garbage(lua: &Lua, limits: LuaExecutionLimits) -> Result<()> {
    if limits.gc_after_execution {
        lua.gc_collect()?;
    }
    Ok(())
}
