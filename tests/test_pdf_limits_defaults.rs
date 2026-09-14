const VARIABLES: [&str; 2] = [
    "IRONFLOW_MAX_PDF_DECOMPRESSED_STREAM_BYTES",
    "IRONFLOW_MAX_PDF_OBJECTS",
];

struct Environment(Vec<Option<std::ffi::OsString>>);

impl Drop for Environment {
    fn drop(&mut self) {
        for (name, previous) in VARIABLES.into_iter().zip(&self.0) {
            // SAFETY: this test binary has one test and restores its environment.
            unsafe {
                match previous {
                    Some(value) => std::env::set_var(name, value),
                    None => std::env::remove_var(name),
                }
            }
        }
    }
}

#[test]
fn pdf_loader_limits_have_nonzero_defaults_and_explicit_overrides() {
    let _environment = Environment(VARIABLES.iter().map(std::env::var_os).collect());
    for value in [None, Some("0"), Some("invalid"), Some("-1"), Some("2048")] {
        for name in VARIABLES {
            // SAFETY: no other tests or threads run in this dedicated binary.
            unsafe {
                match value {
                    Some(value) => std::env::set_var(name, value),
                    None => std::env::remove_var(name),
                }
            }
        }
        let expected = if value == Some("2048") {
            (2048, 2048)
        } else {
            (64 * 1024 * 1024, 250_000)
        };
        assert_eq!(
            ironflow::util::limits::max_pdf_decompressed_stream_bytes(),
            expected.0
        );
        assert_eq!(ironflow::util::limits::max_pdf_objects(), expected.1);
    }
}
