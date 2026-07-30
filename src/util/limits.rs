//! Process-wide size limits for I/O-heavy nodes and embedded runtimes.
//!
//! Each limit is overrideable via environment variable so deployments can tune
//! them without recompiling. Every read path should consult these before
//! allocating unbounded amounts of memory.

mod lua;

pub use lua::{
    LuaExecutionLimits, apply_lua_limits, apply_lua_limits_with_control, collect_lua_garbage,
};

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

/// Whisper-style transcription APIs reject uploads above 25 MB. This is decimal
/// 25 MB, not 25 MiB: a larger default would pass our pre-flight only for the
/// provider to reject the request.
const DEFAULT_MAX_AUDIO_BYTES: u64 = 25_000_000;

/// Default nesting depth accepted when converting between JSON and Lua.
const DEFAULT_MAX_CONVERSION_DEPTH: u64 = 64;

/// Default total value count accepted when converting between JSON and Lua.
/// A flow that legitimately builds a large structure needs a way to raise this
/// without recompiling (IF-058).
const DEFAULT_MAX_CONVERSION_NODES: u64 = 100_000;

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

pub fn max_audio_bytes() -> u64 {
    env_u64("IRONFLOW_MAX_AUDIO_BYTES", DEFAULT_MAX_AUDIO_BYTES)
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

/// Maximum serialized size of a single task's persisted `output` field.
/// Outputs larger than this are replaced with a truncation marker before
/// hitting the storage layer. Default: 2 MB.
const DEFAULT_TASK_OUTPUT_BYTES: u64 = 2 * 1024 * 1024;

/// Maximum rows read from a single worksheet, counting the header row.
const DEFAULT_MAX_XLSX_ROWS: u64 = 50_000;

/// Maximum cells read across every sheet one extraction covers.
///
/// This must fire before `IRONFLOW_MAX_CONVERSION_NODES` (default 100,000)
/// does, or the xlsx ceiling never gets a chance to raise its own
/// sheet-naming error — the parse would already have succeeded and the
/// oversized result would blow up later inside the JSON-to-Lua converter
/// with a message naming a JSON path instead of a sheet (IF-058). Conversion
/// cost for the extracted table is roughly `rows * (cols + 1)` (one node per
/// cell plus one per row for the row wrapper), so a cell ceiling above
/// roughly a third of the conversion budget can be evaded by wide-but-short
/// or narrow-but-long sheets before conversion ever gets involved. 50,000 is
/// chosen to stay well clear of that crossover; if
/// `IRONFLOW_MAX_CONVERSION_NODES` is raised, this should be re-checked
/// against it rather than assumed safe.
const DEFAULT_MAX_XLSX_CELLS: u64 = 50_000;

pub fn max_task_output_bytes() -> u64 {
    env_u64("IRONFLOW_MAX_TASK_OUTPUT_BYTES", DEFAULT_TASK_OUTPUT_BYTES)
}

pub fn max_xlsx_rows() -> u64 {
    env_u64("IRONFLOW_MAX_XLSX_ROWS", DEFAULT_MAX_XLSX_ROWS)
}

pub fn max_xlsx_cells() -> u64 {
    env_u64("IRONFLOW_MAX_XLSX_CELLS", DEFAULT_MAX_XLSX_CELLS)
}
