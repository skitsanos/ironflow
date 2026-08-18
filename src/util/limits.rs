//! Process-wide size limits for I/O-heavy nodes and embedded runtimes.
//!
//! Limits are environment-overridable so deployments can tune them without recompiling.
//! Read paths should consult them before allocating unbounded amounts of memory.
mod image;
mod lua;
mod pdf;
mod xlsx;

pub use image::*;
pub use lua::{
    LuaExecutionLimits, apply_lua_limits, apply_lua_limits_with_control, collect_lua_garbage,
};
pub use pdf::*;
pub use xlsx::{
    max_xlsx_archive_metadata_bytes, max_xlsx_cells, max_xlsx_output_bytes, max_xlsx_rows,
};

/// Default cap for HTTP response bodies (50 MB).
const DEFAULT_HTTP_BODY_BYTES: u64 = 50 * 1024 * 1024;

/// Default cap for LLM provider response bodies (25 MB).
const DEFAULT_LLM_RESPONSE_BYTES: u64 = 25 * 1024 * 1024;

/// Default cumulative raw image-artifact bytes admitted to one LLM request.
const DEFAULT_LLM_IMAGE_INPUT_BYTES: u64 = 50 * 1024 * 1024;

/// Default number of image-artifact blocks admitted to one LLM request.
const DEFAULT_LLM_IMAGE_ARTIFACTS: u64 = 32;

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

/// Default cumulative serialized output cap for document extraction (50 MiB).
const DEFAULT_MAX_EXTRACT_OUTPUT_BYTES: u64 = 50 * 1024 * 1024;

/// Default structural/work-item cap for one document extraction.
const DEFAULT_MAX_EXTRACT_ITEMS: u64 = 250_000;

/// Whisper-style transcription APIs reject uploads above 25 MB. This is decimal
/// 25 MB, not 25 MiB: a larger default would pass our pre-flight only for the
/// provider to reject the request.
const DEFAULT_MAX_AUDIO_BYTES: u64 = 25_000_000;

/// Default cap for transcription provider response bodies (25 MiB).
///
/// Unlike the upload limit above, providers do not define one universal
/// response ceiling. This process-side bound prevents an arbitrary
/// OpenAI-compatible endpoint from making IronFlow buffer an unbounded error
/// or transcript before parsing it or writing `output_file`.
const DEFAULT_MAX_TRANSCRIBE_RESPONSE_BYTES: u64 = 25 * 1024 * 1024;

/// Default nesting depth accepted when converting between JSON and Lua.
const DEFAULT_MAX_CONVERSION_DEPTH: u64 = 64;

/// Default total value count accepted when converting between JSON and Lua.
/// A flow that legitimately builds a large structure needs a way to raise this
/// without recompiling (IF-058).
const DEFAULT_MAX_CONVERSION_NODES: u64 = 100_000;

/// Default Lua instruction budget per Lua state.
const DEFAULT_LUA_MAX_INSTRUCTIONS: u64 = 5_000_000;

/// Default Lua wall-clock budget per Lua state.
const DEFAULT_LUA_MAX_SECONDS: u64 = 10;

/// Default Lua VM memory cap (128 MB).
const DEFAULT_LUA_MAX_MEMORY_BYTES: u64 = 128 * 1024 * 1024;

/// Default cap for Lua flow source files and inline source (1 MiB).
const DEFAULT_MAX_FLOW_SOURCE_BYTES: u64 = 1024 * 1024;

/// How often the Lua debug hook checks budgets.
const DEFAULT_LUA_HOOK_INTERVAL: u64 = 10_000;

pub(super) fn env_u64(var: &str, default: u64) -> u64 {
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

pub fn max_llm_image_input_bytes() -> u64 {
    env_u64(
        "IRONFLOW_LLM_MAX_IMAGE_INPUT_BYTES",
        DEFAULT_LLM_IMAGE_INPUT_BYTES,
    )
}

pub fn max_llm_image_artifacts() -> usize {
    env_u64(
        "IRONFLOW_LLM_MAX_IMAGE_ARTIFACTS",
        DEFAULT_LLM_IMAGE_ARTIFACTS,
    )
    .try_into()
    .unwrap_or(usize::MAX)
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

pub fn max_extract_output_bytes() -> u64 {
    env_u64(
        "IRONFLOW_MAX_EXTRACT_OUTPUT_BYTES",
        DEFAULT_MAX_EXTRACT_OUTPUT_BYTES,
    )
}

pub fn max_extract_items() -> u64 {
    env_u64("IRONFLOW_MAX_EXTRACT_ITEMS", DEFAULT_MAX_EXTRACT_ITEMS)
}

pub fn max_audio_bytes() -> u64 {
    env_u64("IRONFLOW_MAX_AUDIO_BYTES", DEFAULT_MAX_AUDIO_BYTES)
}

/// Maximum response bytes accepted from a transcription provider.
///
/// Invalid and zero values fall back to the safe default rather than disabling
/// the ceiling.
pub fn max_transcribe_response_bytes() -> u64 {
    env_u64(
        "IRONFLOW_MAX_TRANSCRIBE_RESPONSE_BYTES",
        DEFAULT_MAX_TRANSCRIBE_RESPONSE_BYTES,
    )
}

pub fn max_conversion_depth() -> u64 {
    env_u64(
        "IRONFLOW_MAX_CONVERSION_DEPTH",
        DEFAULT_MAX_CONVERSION_DEPTH,
    )
}

pub fn max_conversion_nodes() -> u64 {
    env_u64(
        "IRONFLOW_MAX_CONVERSION_NODES",
        DEFAULT_MAX_CONVERSION_NODES,
    )
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

/// Maximum UTF-8 bytes accepted for one Lua flow source.
///
/// Invalid and zero values retain the safe default instead of disabling the
/// ceiling.
pub fn max_flow_source_bytes() -> u64 {
    env_u64(
        "IRONFLOW_MAX_FLOW_SOURCE_BYTES",
        DEFAULT_MAX_FLOW_SOURCE_BYTES,
    )
}

pub fn lua_hook_interval() -> u64 {
    env_u64("IRONFLOW_LUA_HOOK_INTERVAL", DEFAULT_LUA_HOOK_INTERVAL)
}

pub fn lua_gc_after_execution() -> bool {
    env_bool("IRONFLOW_LUA_GC_AFTER_EXECUTION", true)
}

/// Maximum serialized size of a single task's persisted `output` field.
/// Outputs larger than this are replaced with a truncation marker before
/// hitting the storage layer. Default: 2 MB.
const DEFAULT_TASK_OUTPUT_BYTES: u64 = 2 * 1024 * 1024;

pub fn max_task_output_bytes() -> u64 {
    env_u64("IRONFLOW_MAX_TASK_OUTPUT_BYTES", DEFAULT_TASK_OUTPUT_BYTES)
}
