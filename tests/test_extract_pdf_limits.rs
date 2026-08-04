use std::path::{Path, PathBuf};

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;

const PAGE_LIMIT: &str = "IRONFLOW_MAX_PDF_EXTRACT_PAGES";
const ITEM_LIMIT: &str = "IRONFLOW_MAX_EXTRACT_ITEMS";
const OUTPUT_LIMIT: &str = "IRONFLOW_MAX_EXTRACT_OUTPUT_BYTES";

struct Environment {
    original: Vec<(&'static str, Option<std::ffi::OsString>)>,
}

impl Environment {
    fn capture(names: &[&'static str]) -> Self {
        Self {
            original: names
                .iter()
                .map(|name| (*name, std::env::var_os(name)))
                .collect(),
        }
    }

    fn set(name: &'static str, value: &str) {
        unsafe { std::env::set_var(name, value) };
    }
}

impl Drop for Environment {
    fn drop(&mut self) {
        for (name, value) in &self.original {
            unsafe {
                match value {
                    Some(value) => std::env::set_var(name, value),
                    None => std::env::remove_var(name),
                }
            }
        }
    }
}

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/fixtures/ironflow-sample.pdf")
}

async fn execute(
    path: &Path,
    metadata: bool,
) -> anyhow::Result<ironflow::engine::types::NodeOutput> {
    let node = NodeRegistry::with_builtins().get("extract_pdf").unwrap();
    let mut config = serde_json::json!({
        "path": path.to_string_lossy(),
        "output_key": "content"
    });
    if metadata {
        config["metadata_key"] = serde_json::json!("metadata");
    }
    node.execute(&config, &Context::new()).await
}

#[tokio::test]
async fn extract_pdf_enforces_resource_and_regular_file_boundaries() {
    let _environment = Environment::capture(&[PAGE_LIMIT, ITEM_LIMIT, OUTPUT_LIMIT]);
    Environment::set(PAGE_LIMIT, "1000");
    Environment::set(ITEM_LIMIT, "250000");
    Environment::set(OUTPUT_LIMIT, "52428800");

    let node = NodeRegistry::with_builtins().get("extract_pdf").unwrap();
    let error = node
        .execute(
            &serde_json::json!({
                "path": fixture().to_string_lossy(),
                "metadata_key": false
            }),
            &Context::new(),
        )
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("'metadata_key' must be a string"), "{error}");

    Environment::set(PAGE_LIMIT, "2");
    let error = execute(&fixture(), false).await.unwrap_err().to_string();
    assert!(error.contains(PAGE_LIMIT), "{error}");

    Environment::set(PAGE_LIMIT, "1000");
    Environment::set(ITEM_LIMIT, "2");
    let error = execute(&fixture(), false).await.unwrap_err().to_string();
    assert!(error.contains(ITEM_LIMIT), "{error}");

    Environment::set(ITEM_LIMIT, "250000");
    Environment::set(OUTPUT_LIMIT, "64");
    let error = execute(&fixture(), false).await.unwrap_err().to_string();
    assert!(error.contains(OUTPUT_LIMIT), "{error}");
    assert!(error.contains("page 1"), "{error}");

    Environment::set(OUTPUT_LIMIT, "52428800");
    let directory = tempfile::tempdir().unwrap();
    let malformed = directory.path().join("invalid-info.pdf");
    let mut document = lopdf::Document::load(fixture()).unwrap();
    document.trailer.set("Info", lopdf::Object::Integer(7));
    document.save(&malformed).unwrap();
    let error = execute(&malformed, true).await.unwrap_err().to_string();
    assert!(error.contains("Info entry is not a dictionary"), "{error}");

    #[cfg(unix)]
    {
        use std::ffi::CString;
        use std::os::unix::ffi::OsStrExt;
        use std::os::unix::fs::symlink;

        let link = directory.path().join("linked.pdf");
        symlink(fixture(), &link).unwrap();
        let error = execute(&link, false).await.unwrap_err().to_string();
        assert!(error.contains("failed to open file"), "{error}");

        let fifo = directory.path().join("input.pipe");
        let fifo_c = CString::new(fifo.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(fifo_c.as_ptr(), 0o600) }, 0);
        let result = tokio::time::timeout(std::time::Duration::from_secs(1), execute(&fifo, false))
            .await
            .expect("extract_pdf must reject a FIFO without blocking");
        let error = result.unwrap_err().to_string();
        assert!(error.contains("not a regular file"), "{error}");
    }
}
