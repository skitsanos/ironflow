// The audio cap default lives in its own test binary: the assertion clears a
// process-global environment variable, and `cargo test --lib` runs many test
// modules across threads in a single process where that would race.

#[test]
fn max_audio_bytes_defaults_to_the_provider_limit() {
    let saved = std::env::var("IRONFLOW_MAX_AUDIO_BYTES").ok();
    unsafe { std::env::remove_var("IRONFLOW_MAX_AUDIO_BYTES") };

    let value = ironflow::util::limits::max_audio_bytes();

    if let Some(previous) = saved {
        unsafe { std::env::set_var("IRONFLOW_MAX_AUDIO_BYTES", previous) };
    }

    assert_eq!(value, 25_000_000);
}

#[test]
fn xlsx_ceilings_default_and_override() {
    assert_eq!(ironflow::util::limits::max_xlsx_rows(), 50_000);
    assert_eq!(ironflow::util::limits::max_xlsx_cells(), 50_000);
}
