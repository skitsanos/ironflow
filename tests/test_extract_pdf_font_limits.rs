//! IF-178: the ToUnicode entry budget is configurable and exact at its boundary.

#[path = "pdf_cmap/fixture.rs"]
mod fixture;

use fixture::{extract, range_pdf};
use std::ffi::OsString;

const CMAP_LIMIT: &str = "IRONFLOW_MAX_PDF_EXTRACT_CMAP_ENTRIES";

struct Environment(Option<OsString>);

impl Environment {
    fn set(value: &str) {
        // The serial binary runs one environment-mutating test at a time.
        unsafe { std::env::set_var(CMAP_LIMIT, value) };
    }
}

impl Drop for Environment {
    fn drop(&mut self) {
        unsafe {
            match &self.0 {
                Some(value) => std::env::set_var(CMAP_LIMIT, value),
                None => std::env::remove_var(CMAP_LIMIT),
            }
        }
    }
}

#[tokio::test]
async fn the_cmap_entry_limit_is_configurable_and_exact() {
    let _environment = Environment(std::env::var_os(CMAP_LIMIT));
    let narrow = || range_pdf("0001", "00FF", vec![0, 1, 0, 2, 0, 3]);

    // 255 codes in the range and one codespace line.
    Environment::set("256");
    assert_eq!(extract(&mut narrow()).await.unwrap(), "ABC\n");

    Environment::set("255");
    let error = extract(&mut narrow()).await.unwrap_err().to_string();
    assert!(error.contains(CMAP_LIMIT), "{error}");
    assert!(error.contains("(255)"), "{error}");

    // The same boundary for a three-byte range: 2^17 codes, and a codespace
    // line of codes wider than two bytes counts two.
    Environment::set("131074");
    let mut wide = range_pdf("000000", "01FFFF", vec![0, 0, 2]);
    assert_eq!(extract(&mut wide).await.unwrap(), "C\n");
    Environment::set("131073");
    let error = extract(&mut wide).await.unwrap_err().to_string();
    assert!(error.contains(CMAP_LIMIT), "{error}");
}
