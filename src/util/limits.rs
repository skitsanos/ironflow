//! Process-wide size limits for I/O-heavy nodes.
//!
//! Each limit is overrideable via environment variable so deployments can tune
//! them without recompiling. Every read path should consult these before
//! allocating unbounded amounts of memory.

/// Default cap for HTTP response bodies (50 MB).
const DEFAULT_HTTP_BODY_BYTES: u64 = 50 * 1024 * 1024;

/// Default cap for `read_file` / `write_file` payload size (50 MB).
const DEFAULT_FILE_BYTES: u64 = 50 * 1024 * 1024;

/// Default cap for captured shell `stdout`/`stderr` (10 MB each).
const DEFAULT_SHELL_OUTPUT_BYTES: u64 = 10 * 1024 * 1024;

fn env_u64(var: &str, default: u64) -> u64 {
    std::env::var(var)
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|n| *n > 0)
        .unwrap_or(default)
}

pub fn max_http_body_bytes() -> u64 {
    env_u64("IRONFLOW_MAX_HTTP_BODY_BYTES", DEFAULT_HTTP_BODY_BYTES)
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

/// Maximum serialized size of a single task's persisted `output` field.
/// Outputs larger than this are replaced with a truncation marker before
/// hitting the storage layer. Default: 2 MB.
const DEFAULT_TASK_OUTPUT_BYTES: u64 = 2 * 1024 * 1024;

pub fn max_task_output_bytes() -> u64 {
    env_u64("IRONFLOW_MAX_TASK_OUTPUT_BYTES", DEFAULT_TASK_OUTPUT_BYTES)
}
