//! Process-wide size limits for I/O-heavy nodes and embedded runtimes.
//!
//! Each limit is overrideable via environment variable so deployments can tune
//! them without recompiling. Every read path should consult these before
//! allocating unbounded amounts of memory.

use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{Duration, Instant};

use anyhow::Result;
use mlua::prelude::*;

/// Default cap for HTTP response bodies (50 MB).
const DEFAULT_HTTP_BODY_BYTES: u64 = 50 * 1024 * 1024;

/// Default cap for LLM provider response bodies (25 MB).
const DEFAULT_LLM_RESPONSE_BYTES: u64 = 25 * 1024 * 1024;

/// Default cap for `read_file` / `write_file` payload size (50 MB).
const DEFAULT_FILE_BYTES: u64 = 50 * 1024 * 1024;

/// Default cap for captured shell `stdout`/`stderr` (10 MB each).
const DEFAULT_SHELL_OUTPUT_BYTES: u64 = 10 * 1024 * 1024;

/// Default cap for `db_query` row count.
const DEFAULT_DB_MAX_ROWS: u64 = 1_000;

/// Default cap for serialized `db_query` JSON rows (10 MB).
const DEFAULT_DB_MAX_RESULT_BYTES: u64 = 10 * 1024 * 1024;

/// Default cap for directory listings and ZIP entry enumeration.
const DEFAULT_MAX_DIRECTORY_ENTRIES: u64 = 10_000;

/// Default cap for recursive directory traversal depth.
const DEFAULT_MAX_DIRECTORY_DEPTH: u64 = 32;

/// Default cap for ZIP archive entries.
const DEFAULT_MAX_ZIP_ENTRIES: u64 = 10_000;

/// Default cap for total ZIP uncompressed bytes (512 MB).
const DEFAULT_MAX_ZIP_UNCOMPRESSED_BYTES: u64 = 512 * 1024 * 1024;

/// Default cap for PDF files loaded for rendering (100 MB).
const DEFAULT_MAX_PDF_BYTES: u64 = 100 * 1024 * 1024;

/// Default cap for PDF pages rendered into base64 in one node call.
const DEFAULT_MAX_PDF_RENDER_PAGES: u64 = 25;

/// Default cap for a rendered PDF page's pixels (25 megapixels).
const DEFAULT_MAX_PDF_RENDER_PIXELS: u64 = 25_000_000;

/// Default cap for PDF render DPI.
const DEFAULT_MAX_PDF_DPI: u64 = 300;

/// Default Lua instruction budget per Lua state.
const DEFAULT_LUA_MAX_INSTRUCTIONS: u64 = 5_000_000;

/// Default Lua wall-clock budget per Lua state.
const DEFAULT_LUA_MAX_SECONDS: u64 = 10;

/// Default Lua VM memory cap (128 MB).
const DEFAULT_LUA_MAX_MEMORY_BYTES: u64 = 128 * 1024 * 1024;

/// How often the Lua debug hook checks budgets.
const DEFAULT_LUA_HOOK_INTERVAL: u64 = 10_000;

fn env_u64(var: &str, default: u64) -> u64 {
    std::env::var(var)
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|n| *n > 0)
        .unwrap_or(default)
}

fn env_optional_u64(var: &str, default: u64) -> Option<u64> {
    let value = std::env::var(var)
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(default);

    (value > 0).then_some(value)
}

fn env_bool(var: &str, default: bool) -> bool {
    std::env::var(var)
        .ok()
        .and_then(|v| v.parse::<bool>().ok())
        .unwrap_or(default)
}

pub fn max_http_body_bytes() -> u64 {
    env_u64("IRONFLOW_MAX_HTTP_BODY_BYTES", DEFAULT_HTTP_BODY_BYTES)
}

pub fn max_llm_response_bytes() -> Option<u64> {
    env_optional_u64(
        "IRONFLOW_LLM_MAX_RESPONSE_BYTES",
        DEFAULT_LLM_RESPONSE_BYTES,
    )
}

pub fn max_file_bytes() -> u64 {
    env_u64("IRONFLOW_MAX_FILE_BYTES", DEFAULT_FILE_BYTES)
}

pub fn max_shell_output_bytes() -> u64 {
    env_u64(
        "IRONFLOW_MAX_SHELL_OUTPUT_BYTES",
        DEFAULT_SHELL_OUTPUT_BYTES,
    )
}

pub fn max_db_rows() -> Option<u64> {
    env_optional_u64("IRONFLOW_DB_MAX_ROWS", DEFAULT_DB_MAX_ROWS)
}

pub fn max_db_result_bytes() -> Option<u64> {
    env_optional_u64("IRONFLOW_DB_MAX_RESULT_BYTES", DEFAULT_DB_MAX_RESULT_BYTES)
}

pub fn max_directory_entries() -> u64 {
    env_u64(
        "IRONFLOW_MAX_DIRECTORY_ENTRIES",
        DEFAULT_MAX_DIRECTORY_ENTRIES,
    )
}

pub fn max_directory_depth() -> u64 {
    env_u64("IRONFLOW_MAX_DIRECTORY_DEPTH", DEFAULT_MAX_DIRECTORY_DEPTH)
}

pub fn max_zip_entries() -> u64 {
    env_u64("IRONFLOW_MAX_ZIP_ENTRIES", DEFAULT_MAX_ZIP_ENTRIES)
}

pub fn max_zip_uncompressed_bytes() -> u64 {
    env_u64(
        "IRONFLOW_MAX_ZIP_UNCOMPRESSED_BYTES",
        DEFAULT_MAX_ZIP_UNCOMPRESSED_BYTES,
    )
}

pub fn max_pdf_bytes() -> u64 {
    env_u64("IRONFLOW_MAX_PDF_BYTES", DEFAULT_MAX_PDF_BYTES)
}

pub fn max_pdf_render_pages() -> u64 {
    env_u64(
        "IRONFLOW_MAX_PDF_RENDER_PAGES",
        DEFAULT_MAX_PDF_RENDER_PAGES,
    )
}

pub fn max_pdf_render_pixels() -> u64 {
    env_u64(
        "IRONFLOW_MAX_PDF_RENDER_PIXELS",
        DEFAULT_MAX_PDF_RENDER_PIXELS,
    )
}

pub fn max_pdf_dpi() -> u64 {
    env_u64("IRONFLOW_MAX_PDF_DPI", DEFAULT_MAX_PDF_DPI)
}

pub fn max_lua_instructions() -> Option<u64> {
    env_optional_u64(
        "IRONFLOW_LUA_MAX_INSTRUCTIONS",
        DEFAULT_LUA_MAX_INSTRUCTIONS,
    )
}

pub fn max_lua_seconds() -> Option<u64> {
    env_optional_u64("IRONFLOW_LUA_MAX_SECONDS", DEFAULT_LUA_MAX_SECONDS)
}

pub fn max_lua_memory_bytes() -> Option<u64> {
    env_optional_u64(
        "IRONFLOW_LUA_MAX_MEMORY_BYTES",
        DEFAULT_LUA_MAX_MEMORY_BYTES,
    )
}

pub fn lua_hook_interval() -> u64 {
    env_u64("IRONFLOW_LUA_HOOK_INTERVAL", DEFAULT_LUA_HOOK_INTERVAL)
}

pub fn lua_gc_after_execution() -> bool {
    env_bool("IRONFLOW_LUA_GC_AFTER_EXECUTION", true)
}

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
    lua.gc_restart();
    lua.gc_inc(200, 200, 13);

    if let Some(max_memory_bytes) = limits.max_memory_bytes {
        lua.set_memory_limit(max_memory_bytes as usize)?;
    }

    let hook_interval = limits.hook_interval.max(1);
    if limits.max_instructions.is_none() && limits.max_seconds.is_none() {
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

/// Maximum serialized size of a single task's persisted `output` field.
/// Outputs larger than this are replaced with a truncation marker before
/// hitting the storage layer. Default: 2 MB.
const DEFAULT_TASK_OUTPUT_BYTES: u64 = 2 * 1024 * 1024;

pub fn max_task_output_bytes() -> u64 {
    env_u64("IRONFLOW_MAX_TASK_OUTPUT_BYTES", DEFAULT_TASK_OUTPUT_BYTES)
}
