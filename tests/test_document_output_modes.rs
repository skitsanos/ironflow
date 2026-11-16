//! IF-185: the Markdown document nodes, `pdf_merge` and grouped `pdf_split`
//! publish through the staged writer with the mode a plain write would have
//! left: the platform default for a new file, the existing permission bits
//! for a replaced one.
#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use ironflow::engine::types::{Context, NodeOutput};
use ironflow::nodes::NodeRegistry;
use serde_json::{Value, json};

fn scratch() -> (tempfile::TempDir, PathBuf) {
    let directory = tempfile::tempdir().unwrap();
    let work = directory.path().join("work");
    std::fs::create_dir(&work).unwrap();
    (directory, work)
}

fn mode(path: &Path) -> u32 {
    std::fs::metadata(path).unwrap().permissions().mode() & 0o7777
}

/// The mode a plain `File::create` leaves in `directory` under this
/// process's umask, measured instead of assumed.
fn default_mode(directory: &Path) -> u32 {
    let probe = directory.join("plain-create-probe");
    std::fs::File::create(&probe).unwrap();
    let measured = mode(&probe);
    std::fs::remove_file(&probe).unwrap();
    measured
}

fn existing_with_mode(path: &Path, mode: u32) {
    std::fs::write(path, b"previous output").unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
}

async fn execute(node: &str, config: Value) -> NodeOutput {
    NodeRegistry::with_builtins()
        .get(node)
        .unwrap()
        .execute(&config, &Context::new())
        .await
        .unwrap_or_else(|error| panic!("{node} failed: {error:#}"))
}

/// A one-page PDF produced by the node under test's sibling.
async fn one_page_pdf(path: &Path) {
    execute(
        "markdown_to_pdf",
        json!({"input": "# Page\n\nBody.", "output_path": path}),
    )
    .await;
}

#[tokio::test]
async fn markdown_documents_follow_the_mode_contract() {
    for (node, extension) in [("markdown_to_pdf", "pdf"), ("markdown_to_docx", "docx")] {
        let (_directory, work) = scratch();
        let expected = default_mode(&work);
        let created = work.join(format!("created.{extension}"));
        let replaced = work.join(format!("replaced.{extension}"));
        existing_with_mode(&replaced, 0o640);

        execute(node, json!({"input": "# Report", "output_path": created})).await;
        execute(
            node,
            json!({"input": "# Report", "output_path": replaced, "overwrite": true}),
        )
        .await;

        assert_eq!(
            format!("{:o}", mode(&created)),
            format!("{expected:o}"),
            "{node}: a new document must have the mode of a plain create"
        );
        assert_ne!(std::fs::read(&replaced).unwrap(), b"previous output");
        assert_eq!(
            format!("{:o}", mode(&replaced)),
            "640",
            "{node}: a replaced document must keep its permission bits"
        );
    }
}

#[tokio::test]
async fn pdf_merge_follows_the_mode_contract() {
    let (directory, work) = scratch();
    let expected = default_mode(&work);
    let source = directory.path().join("page.pdf");
    one_page_pdf(&source).await;
    let created = work.join("created.pdf");
    let replaced = work.join("replaced.pdf");
    existing_with_mode(&replaced, 0o640);

    for output_path in [&created, &replaced] {
        execute(
            "pdf_merge",
            json!({"files": [source, source], "output_path": output_path}),
        )
        .await;
    }

    assert_eq!(
        format!("{:o}", mode(&created)),
        format!("{expected:o}"),
        "a new merged PDF must have the mode of a plain create"
    );
    assert!(std::fs::read(&replaced).unwrap().starts_with(b"%PDF"));
    assert_eq!(
        format!("{:o}", mode(&replaced)),
        "640",
        "a replaced merged PDF must keep its permission bits"
    );
}

#[tokio::test]
async fn grouped_pdf_split_parts_get_the_default_mode() {
    let (directory, work) = scratch();
    let expected = default_mode(&work);
    let page = directory.path().join("page.pdf");
    one_page_pdf(&page).await;
    let source = directory.path().join("four-pages.pdf");
    execute(
        "pdf_merge",
        json!({"files": [page, page, page, page], "output_path": source}),
    )
    .await;

    let output = execute(
        "pdf_split",
        json!({"path": source, "output_dir": work, "pages_per_file": 2}),
    )
    .await;

    let parts = output["pdf_split_files"].as_array().unwrap();
    assert_eq!(parts.len(), 2, "{output:?}");
    for part in parts {
        let part = Path::new(part.as_str().unwrap());
        assert_eq!(
            format!("{:o}", mode(part)),
            format!("{expected:o}"),
            "{}: a new part must have the mode of a plain create",
            part.display()
        );
    }
}
