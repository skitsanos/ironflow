use std::path::Path;

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;
use lopdf::{Document, Object, Stream, dictionary};

static ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

struct ArtifactEnvironment(Option<std::ffi::OsString>);

impl ArtifactEnvironment {
    fn set(path: &Path) -> Self {
        let previous = std::env::var_os("IRONFLOW_ARTIFACT_DIR");
        // SAFETY: artifact environment mutation is serialized in this binary.
        unsafe { std::env::set_var("IRONFLOW_ARTIFACT_DIR", path) };
        Self(previous)
    }
}

impl Drop for ArtifactEnvironment {
    fn drop(&mut self) {
        // SAFETY: the environment lock remains held until this guard drops.
        unsafe {
            match self.0.take() {
                Some(value) => std::env::set_var("IRONFLOW_ARTIFACT_DIR", value),
                None => std::env::remove_var("IRONFLOW_ARTIFACT_DIR"),
            }
        }
    }
}

fn create_pdf(path: &Path, label: &str) {
    let mut document = Document::new();
    let pages_id = document.new_object_id();
    let content = document.add_object(Stream::new(
        dictionary! {},
        format!("BT ({label}) Tj ET").into_bytes(),
    ));
    let page = document.add_object(dictionary! {
        "Type" => "Page", "Parent" => pages_id, "Contents" => content,
        "MediaBox" => vec![0.into(), 0.into(), 100.into(), 100.into()]
    });
    document.objects.insert(
        pages_id,
        Object::Dictionary(
            dictionary! { "Type" => "Pages", "Kids" => vec![page.into()], "Count" => 1 },
        ),
    );
    let catalog = document.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    document.trailer.set("Root", catalog);
    document.save(path).unwrap();
}

#[tokio::test]
async fn pdf_merge_accepts_verified_artifact_descriptors() {
    let _lock = ENV_LOCK.lock().await;
    let directory = tempfile::tempdir().unwrap();
    let artifact_dir = directory.path().join("artifacts");
    let _environment = ArtifactEnvironment::set(&artifact_dir);
    let first = directory.path().join("first.pdf");
    let second = directory.path().join("second.pdf");
    let output = directory.path().join("merged.pdf");
    create_pdf(&first, "first");
    create_pdf(&second, "second");
    let registry = NodeRegistry::with_builtins();

    let mut descriptors = Vec::new();
    for source in [first, second] {
        let read = registry
            .get("read_file")
            .unwrap()
            .execute(
                &serde_json::json!({"path": source, "encoding": "artifact"}),
                &Context::new(),
            )
            .await
            .unwrap();
        descriptors.push(read["file_artifact"].clone());
    }
    let context = Context::from([(
        "merge_sources".to_owned(),
        serde_json::Value::Array(descriptors),
    )]);
    let merged = registry
        .get("pdf_merge")
        .unwrap()
        .execute(
            &serde_json::json!({"source_key": "merge_sources", "output_path": output}),
            &context,
        )
        .await
        .unwrap();
    assert_eq!(merged["pdf_merge_page_count"], 2);
    assert_eq!(Document::load(output).unwrap().get_pages().len(), 2);
}
