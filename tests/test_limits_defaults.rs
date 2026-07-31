// The audio cap default lives in its own test binary: the assertion clears a
// process-global environment variable, and `cargo test --lib` runs many test
// modules across threads in a single process where that would race.

#[test]
fn max_audio_bytes_defaults_to_the_provider_limit() {
    let saved = std::env::var("IRONFLOW_MAX_AUDIO_BYTES").ok();
    let saved_response = std::env::var("IRONFLOW_MAX_TRANSCRIBE_RESPONSE_BYTES").ok();
    let saved_flow_source = std::env::var("IRONFLOW_MAX_FLOW_SOURCE_BYTES").ok();
    unsafe {
        std::env::remove_var("IRONFLOW_MAX_AUDIO_BYTES");
        std::env::remove_var("IRONFLOW_MAX_TRANSCRIBE_RESPONSE_BYTES");
        std::env::remove_var("IRONFLOW_MAX_FLOW_SOURCE_BYTES");
    }

    let value = ironflow::util::limits::max_audio_bytes();
    let response_value = ironflow::util::limits::max_transcribe_response_bytes();
    let flow_source_value = ironflow::util::limits::max_flow_source_bytes();

    unsafe {
        if let Some(previous) = saved {
            std::env::set_var("IRONFLOW_MAX_AUDIO_BYTES", previous);
        }
        if let Some(previous) = saved_response {
            std::env::set_var("IRONFLOW_MAX_TRANSCRIBE_RESPONSE_BYTES", previous);
        }
        if let Some(previous) = saved_flow_source {
            std::env::set_var("IRONFLOW_MAX_FLOW_SOURCE_BYTES", previous);
        }
    }

    assert_eq!(value, 25_000_000);
    assert_eq!(response_value, 25 * 1024 * 1024);
    assert_eq!(flow_source_value, 1024 * 1024);
}

#[test]
fn xlsx_ceilings_default_when_unset() {
    // Only defaults are asserted here (no override) -- clear both variables
    // first, following the pattern above, so this doesn't fail on any
    // machine that happens to export one of them.
    let saved_rows = std::env::var("IRONFLOW_MAX_XLSX_ROWS").ok();
    let saved_cells = std::env::var("IRONFLOW_MAX_XLSX_CELLS").ok();
    let saved_output = std::env::var("IRONFLOW_MAX_XLSX_OUTPUT_BYTES").ok();
    unsafe {
        std::env::remove_var("IRONFLOW_MAX_XLSX_ROWS");
        std::env::remove_var("IRONFLOW_MAX_XLSX_CELLS");
        std::env::remove_var("IRONFLOW_MAX_XLSX_OUTPUT_BYTES");
    }

    let rows = ironflow::util::limits::max_xlsx_rows();
    let cells = ironflow::util::limits::max_xlsx_cells();
    let output_bytes = ironflow::util::limits::max_xlsx_output_bytes();

    unsafe {
        if let Some(previous) = saved_rows {
            std::env::set_var("IRONFLOW_MAX_XLSX_ROWS", previous);
        }
        if let Some(previous) = saved_cells {
            std::env::set_var("IRONFLOW_MAX_XLSX_CELLS", previous);
        }
        if let Some(previous) = saved_output {
            std::env::set_var("IRONFLOW_MAX_XLSX_OUTPUT_BYTES", previous);
        }
    }

    assert_eq!(rows, 50_000);
    assert_eq!(cells, 33_000);
    assert_eq!(output_bytes, 50 * 1024 * 1024);
}
