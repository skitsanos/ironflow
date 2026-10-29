//! Process-wide size limits for I/O-heavy nodes and embedded runtimes.
//!
//! Limits are environment-overridable so deployments can tune them without
//! recompiling. Every numeric limit is declared once as a [`Limit`], here or in
//! a submodule, and listed in [`ALL`] so that [`validate_environment`] rejects
//! a malformed override at startup under one contract (see `env.rs`). Read
//! paths consult the accessors before allocating unbounded amounts of memory.
mod env;
mod image;
mod lua;
mod pdf;
mod process;
mod xlsx;

pub use env::{Limit, Toggle, Zero, validate_environment};
pub use image::*;
pub use lua::{
    LuaExecutionLimits, apply_lua_limits, apply_lua_limits_with_control, collect_lua_garbage,
};
pub use pdf::*;
pub use process::*;
pub use xlsx::{
    max_xlsx_archive_metadata_bytes, max_xlsx_cells, max_xlsx_output_bytes, max_xlsx_rows,
};

pub(crate) const MIB: u64 = 1024 * 1024;

/// HTTP response bodies (50 MiB).
pub(crate) const MAX_HTTP_BODY_BYTES: Limit =
    Limit::positive("IRONFLOW_MAX_HTTP_BODY_BYTES", 50 * MIB);
/// Total scalar values across one embedding matrix.
pub(crate) const MAX_EMBEDDING_VALUES: Limit =
    Limit::positive("IRONFLOW_MAX_EMBEDDING_VALUES", 16_777_216);
/// LLM provider response bodies (25 MiB); `0` disables the ceiling.
pub(crate) const LLM_MAX_RESPONSE_BYTES: Limit =
    Limit::optional("IRONFLOW_LLM_MAX_RESPONSE_BYTES", 25 * MIB);
/// Cumulative raw image-artifact bytes admitted to one LLM request (50 MiB).
pub(crate) const LLM_MAX_IMAGE_INPUT_BYTES: Limit =
    Limit::positive("IRONFLOW_LLM_MAX_IMAGE_INPUT_BYTES", 50 * MIB);
/// Image-artifact blocks admitted to one LLM request.
pub(crate) const LLM_MAX_IMAGE_ARTIFACTS: Limit =
    Limit::positive("IRONFLOW_LLM_MAX_IMAGE_ARTIFACTS", 32);
/// `read_file` / `write_file` payload size (50 MiB).
pub(crate) const MAX_FILE_BYTES: Limit = Limit::positive("IRONFLOW_MAX_FILE_BYTES", 50 * MIB);
/// Captured shell `stdout` and `stderr`, each (10 MiB).
pub(crate) const MAX_SHELL_OUTPUT_BYTES: Limit =
    Limit::positive("IRONFLOW_MAX_SHELL_OUTPUT_BYTES", 10 * MIB);
/// `db_query` row count; `0` disables the ceiling.
pub(crate) const DB_MAX_ROWS: Limit = Limit::optional("IRONFLOW_DB_MAX_ROWS", 1_000);
/// Serialized `db_query` JSON rows (10 MiB); `0` disables the ceiling.
pub(crate) const DB_MAX_RESULT_BYTES: Limit =
    Limit::optional("IRONFLOW_DB_MAX_RESULT_BYTES", 10 * MIB);
/// Directory listings and ZIP entry enumeration.
pub(crate) const MAX_DIRECTORY_ENTRIES: Limit =
    Limit::positive("IRONFLOW_MAX_DIRECTORY_ENTRIES", 10_000);
/// Recursive directory traversal depth.
pub(crate) const MAX_DIRECTORY_DEPTH: Limit = Limit::positive("IRONFLOW_MAX_DIRECTORY_DEPTH", 32);
/// ZIP archive entries.
pub(crate) const MAX_ZIP_ENTRIES: Limit = Limit::positive("IRONFLOW_MAX_ZIP_ENTRIES", 10_000);
/// Cumulative raw ZIP names, extra fields and comments (8 MiB).
pub(crate) const MAX_ZIP_METADATA_BYTES: Limit =
    Limit::positive("IRONFLOW_MAX_ZIP_METADATA_BYTES", 8 * MIB);
/// Total ZIP uncompressed bytes (512 MiB).
pub(crate) const MAX_ZIP_UNCOMPRESSED_BYTES: Limit =
    Limit::positive("IRONFLOW_MAX_ZIP_UNCOMPRESSED_BYTES", 512 * MIB);
/// Cumulative serialized output of one document extraction (50 MiB).
pub(crate) const MAX_EXTRACT_OUTPUT_BYTES: Limit =
    Limit::positive("IRONFLOW_MAX_EXTRACT_OUTPUT_BYTES", 50 * MIB);
/// Structural/work-item cap for one document extraction.
pub(crate) const MAX_EXTRACT_ITEMS: Limit = Limit::positive("IRONFLOW_MAX_EXTRACT_ITEMS", 250_000);
/// Whisper-style transcription APIs reject uploads above 25 MB. This is decimal
/// 25 MB, not 25 MiB: a larger default would pass our pre-flight only for the
/// provider to reject the request.
pub(crate) const MAX_AUDIO_BYTES: Limit = Limit::positive("IRONFLOW_MAX_AUDIO_BYTES", 25_000_000);
/// Transcription provider response bodies (25 MiB). Providers define no
/// universal response ceiling; this process-side bound stops an arbitrary
/// OpenAI-compatible endpoint from making IronFlow buffer an unbounded error
/// or transcript before parsing it or writing `output_file`.
pub(crate) const MAX_TRANSCRIBE_RESPONSE_BYTES: Limit =
    Limit::positive("IRONFLOW_MAX_TRANSCRIBE_RESPONSE_BYTES", 25 * MIB);
/// Nesting depth accepted when converting between JSON and Lua.
pub(crate) const MAX_CONVERSION_DEPTH: Limit = Limit::positive("IRONFLOW_MAX_CONVERSION_DEPTH", 64);
/// Total value count accepted when converting between JSON and Lua. A flow
/// that legitimately builds a large structure needs a way to raise this
/// without recompiling (IF-058).
pub(crate) const MAX_CONVERSION_NODES: Limit =
    Limit::positive("IRONFLOW_MAX_CONVERSION_NODES", 100_000);
/// Lua instruction budget per Lua state; `0` disables it.
pub(crate) const LUA_MAX_INSTRUCTIONS: Limit =
    Limit::optional("IRONFLOW_LUA_MAX_INSTRUCTIONS", 5_000_000);
/// Lua wall-clock budget per Lua state, in seconds; `0` disables it.
pub(crate) const LUA_MAX_SECONDS: Limit = Limit::optional("IRONFLOW_LUA_MAX_SECONDS", 10);
/// Lua VM memory cap (128 MiB); `0` disables it.
pub(crate) const LUA_MAX_MEMORY_BYTES: Limit =
    Limit::optional("IRONFLOW_LUA_MAX_MEMORY_BYTES", 128 * MIB);
/// Lua flow source files and inline source (1 MiB).
pub(crate) const MAX_FLOW_SOURCE_BYTES: Limit =
    Limit::positive("IRONFLOW_MAX_FLOW_SOURCE_BYTES", MIB);
/// How often the Lua debug hook checks budgets.
pub(crate) const LUA_HOOK_INTERVAL: Limit = Limit::positive("IRONFLOW_LUA_HOOK_INTERVAL", 10_000);
/// Run a Lua garbage-collection cycle after flow parsing and code execution.
pub(crate) const LUA_GC_AFTER_EXECUTION: Toggle =
    Toggle::new("IRONFLOW_LUA_GC_AFTER_EXECUTION", true);
/// Serialized size of one persisted task `output` (2 MiB); larger outputs are
/// replaced with a truncation marker before they reach the storage layer.
pub(crate) const MAX_TASK_OUTPUT_BYTES: Limit =
    Limit::positive("IRONFLOW_MAX_TASK_OUTPUT_BYTES", 2 * MIB);

/// Every numeric limit, checked by [`validate_environment`] at startup.
pub(crate) const ALL: &[&Limit] = &[
    &MAX_HTTP_BODY_BYTES,
    &MAX_EMBEDDING_VALUES,
    &LLM_MAX_RESPONSE_BYTES,
    &LLM_MAX_IMAGE_INPUT_BYTES,
    &LLM_MAX_IMAGE_ARTIFACTS,
    &MAX_FILE_BYTES,
    &MAX_SHELL_OUTPUT_BYTES,
    &DB_MAX_ROWS,
    &DB_MAX_RESULT_BYTES,
    &MAX_DIRECTORY_ENTRIES,
    &MAX_DIRECTORY_DEPTH,
    &MAX_ZIP_ENTRIES,
    &MAX_ZIP_METADATA_BYTES,
    &MAX_ZIP_UNCOMPRESSED_BYTES,
    &MAX_EXTRACT_OUTPUT_BYTES,
    &MAX_EXTRACT_ITEMS,
    &MAX_AUDIO_BYTES,
    &MAX_TRANSCRIBE_RESPONSE_BYTES,
    &MAX_CONVERSION_DEPTH,
    &MAX_CONVERSION_NODES,
    &LUA_MAX_INSTRUCTIONS,
    &LUA_MAX_SECONDS,
    &LUA_MAX_MEMORY_BYTES,
    &MAX_FLOW_SOURCE_BYTES,
    &LUA_HOOK_INTERVAL,
    &MAX_TASK_OUTPUT_BYTES,
    &image::MAX_IMAGE_ENCODED_BYTES,
    &image::MAX_IMAGE_PIXELS,
    &image::MAX_IMAGE_DECODE_ALLOCATION_BYTES,
    &image::MAX_IMAGE_TO_PDF_SOURCES,
    &image::MAX_IMAGE_TO_PDF_ENCODED_BYTES,
    &image::MAX_IMAGE_TO_PDF_PIXELS,
    &pdf::MAX_PDF_BYTES,
    &pdf::MAX_PDF_DECOMPRESSED_STREAM_BYTES,
    &pdf::MAX_PDF_OBJECTS,
    &pdf::MAX_PDF_EXTRACT_PAGES,
    &pdf::MAX_PDF_RENDER_PAGES,
    &pdf::MAX_PDF_SPLIT_PAGES,
    &pdf::MAX_PDF_RENDER_PIXELS,
    &pdf::MAX_PDF_DPI,
    &pdf::MAX_PDF_MERGE_FILES,
    &pdf::MAX_PDF_MERGE_BYTES,
    &pdf::MAX_PDF_MERGE_PAGES,
    &pdf::MAX_PDF_MERGE_OBJECTS,
    &xlsx::MAX_XLSX_ROWS,
    &xlsx::MAX_XLSX_CELLS,
    &xlsx::MAX_XLSX_OUTPUT_BYTES,
    &xlsx::MAX_XLSX_ARCHIVE_METADATA_BYTES,
    &process::MAX_DETACHED_SUBWORKFLOWS,
    &process::OAUTH_CACHE_SIZE,
    &process::CACHE_MAX_ENTRIES,
    &process::MCP_SESSION_CACHE_SIZE,
    &process::MCP_SESSION_TTL_SECS,
];

/// Every boolean switch, checked by [`validate_environment`] at startup.
pub(crate) const TOGGLES: &[&Toggle] = &[&LUA_GC_AFTER_EXECUTION];

pub fn max_http_body_bytes() -> u64 {
    MAX_HTTP_BODY_BYTES.read()
}

pub fn max_embedding_values() -> u64 {
    MAX_EMBEDDING_VALUES.read()
}

pub fn max_llm_response_bytes() -> Option<u64> {
    LLM_MAX_RESPONSE_BYTES.read_optional()
}

pub fn max_llm_image_input_bytes() -> u64 {
    LLM_MAX_IMAGE_INPUT_BYTES.read()
}

pub fn max_llm_image_artifacts() -> usize {
    usize::try_from(LLM_MAX_IMAGE_ARTIFACTS.read()).unwrap_or(usize::MAX)
}

pub fn max_file_bytes() -> u64 {
    MAX_FILE_BYTES.read()
}

pub fn max_shell_output_bytes() -> u64 {
    MAX_SHELL_OUTPUT_BYTES.read()
}

pub fn max_db_rows() -> Option<u64> {
    DB_MAX_ROWS.read_optional()
}

pub fn max_db_result_bytes() -> Option<u64> {
    DB_MAX_RESULT_BYTES.read_optional()
}

pub fn max_directory_entries() -> u64 {
    MAX_DIRECTORY_ENTRIES.read()
}

pub fn max_directory_depth() -> u64 {
    MAX_DIRECTORY_DEPTH.read()
}

pub fn max_zip_entries() -> u64 {
    MAX_ZIP_ENTRIES.read()
}

pub(crate) fn max_zip_metadata_bytes() -> u64 {
    MAX_ZIP_METADATA_BYTES.read()
}

pub fn max_zip_uncompressed_bytes() -> u64 {
    MAX_ZIP_UNCOMPRESSED_BYTES.read()
}

pub fn max_extract_output_bytes() -> u64 {
    MAX_EXTRACT_OUTPUT_BYTES.read()
}

pub fn max_extract_items() -> u64 {
    MAX_EXTRACT_ITEMS.read()
}

pub fn max_audio_bytes() -> u64 {
    MAX_AUDIO_BYTES.read()
}

/// Maximum response bytes accepted from a transcription provider.
pub fn max_transcribe_response_bytes() -> u64 {
    MAX_TRANSCRIBE_RESPONSE_BYTES.read()
}

pub fn max_conversion_depth() -> u64 {
    MAX_CONVERSION_DEPTH.read()
}

pub fn max_conversion_nodes() -> u64 {
    MAX_CONVERSION_NODES.read()
}

pub fn max_lua_instructions() -> Option<u64> {
    LUA_MAX_INSTRUCTIONS.read_optional()
}

pub fn max_lua_seconds() -> Option<u64> {
    LUA_MAX_SECONDS.read_optional()
}

pub fn max_lua_memory_bytes() -> Option<u64> {
    LUA_MAX_MEMORY_BYTES.read_optional()
}

/// Maximum UTF-8 bytes accepted for one Lua flow source.
pub fn max_flow_source_bytes() -> u64 {
    MAX_FLOW_SOURCE_BYTES.read()
}

pub fn lua_hook_interval() -> u64 {
    LUA_HOOK_INTERVAL.read()
}

pub fn lua_gc_after_execution() -> bool {
    LUA_GC_AFTER_EXECUTION.read()
}

pub fn max_task_output_bytes() -> u64 {
    MAX_TASK_OUTPUT_BYTES.read()
}
