use super::*;
use lopdf::{Stream, dictionary};

fn shared_resource_pdf(path: &Path) {
    let mut document = Document::new();
    let pages_id = document.new_object_id();
    let font_id = document.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica"
    });
    let resources_id = document.add_object(dictionary! {
        "Font" => dictionary! { "F1" => font_id }
    });
    let mut pages = Vec::new();
    for index in 0..2 {
        let content_id = document.add_object(Stream::new(
            dictionary! {},
            format!("BT /F1 12 Tf ({index}) Tj ET").into_bytes(),
        ));
        let page_id = document.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => content_id,
            "Resources" => resources_id,
            "MediaBox" => vec![0.into(), 0.into(), 100.into(), 100.into()],
        });
        pages.push(page_id);
    }
    document.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => pages.iter().copied().map(Object::Reference).collect::<Vec<_>>(),
            "Count" => 2,
        }),
    );
    let catalog = document.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    document.trailer.set("Root", catalog);
    document.save(path).unwrap();
}

fn limits(objects: u64, pages: u64, bytes: u64) -> Limits {
    Limits {
        files: 2,
        per_file_bytes: bytes,
        total_bytes: bytes,
        pages,
        objects,
    }
}

#[test]
fn file_count_is_admitted_before_sources_are_cloned() {
    let config = serde_json::json!({"files": ["a.pdf", "b.pdf"]});
    let error = parse_sources(&config, &Context::new(), 1)
        .unwrap_err()
        .to_string();
    assert!(error.contains("IRONFLOW_MAX_PDF_MERGE_FILES"), "{error}");
}

#[tokio::test]
async fn shared_resources_are_retained_once_across_pages() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("source.pdf");
    let output = directory.path().join("merged.pdf");
    shared_resource_pdf(&source);
    let source_bytes = std::fs::metadata(&source).unwrap().len();
    let request = Request {
        sources: vec![FileSource::path(source)],
        output_path: output.clone(),
        limits: limits(8, 2, source_bytes.saturating_mul(2)),
    };
    let count = run_tracked_blocking_step(move |execution| merge(request, &execution))
        .await
        .unwrap();
    assert_eq!(count, 2);
    let merged = Document::load(output).unwrap();
    let font_count = merged
        .objects
        .values()
        .filter(|object| {
            object
                .as_dict()
                .ok()
                .and_then(|dictionary| dictionary.get(b"Type").ok())
                .and_then(|value| value.as_name().ok())
                == Some(b"Font".as_slice())
        })
        .count();
    assert_eq!(font_count, 1, "shared font was cloned per page");
}

#[tokio::test]
async fn limit_or_malformed_input_preserves_existing_output() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("source.pdf");
    let malformed = directory.path().join("malformed.pdf");
    let output = directory.path().join("merged.pdf");
    shared_resource_pdf(&source);
    std::fs::write(&malformed, b"not a pdf").unwrap();
    std::fs::write(&output, b"original").unwrap();
    let bytes = std::fs::metadata(&source).unwrap().len().saturating_mul(4);
    let request = Request {
        sources: vec![FileSource::path(source), FileSource::path(malformed)],
        output_path: output.clone(),
        limits: limits(100, 10, bytes),
    };
    assert!(
        run_tracked_blocking_step(move |execution| merge(request, &execution))
            .await
            .is_err()
    );
    assert_eq!(std::fs::read(&output).unwrap(), b"original");
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 3);
}

#[tokio::test]
async fn cumulative_page_limit_fails_before_output_staging() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("source.pdf");
    let output = directory.path().join("merged.pdf");
    shared_resource_pdf(&source);
    let bytes = std::fs::metadata(&source).unwrap().len().saturating_mul(2);
    let request = Request {
        sources: vec![FileSource::path(source)],
        output_path: output.clone(),
        limits: limits(100, 1, bytes),
    };
    let error = run_tracked_blocking_step(move |execution| merge(request, &execution))
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("IRONFLOW_MAX_PDF_MERGE_PAGES"), "{error}");
    assert!(!output.exists());
}

#[tokio::test]
async fn output_limit_failure_removes_partial_staging_and_preserves_destination() {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("merged.pdf");
    std::fs::write(&output, b"original").unwrap();
    let worker_output = output.clone();
    let error = run_tracked_blocking_step(move |execution| {
        let mut document = Document::new();
        let pages = document.new_object_id();
        document.objects.insert(
            pages,
            Object::Dictionary(
                dictionary! { "Type" => "Pages", "Kids" => Vec::<Object>::new(), "Count" => 0 },
            ),
        );
        let catalog = document.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages });
        document.trailer.set("Root", catalog);
        save_atomic(&mut document, &worker_output, 1, &execution)
    })
    .await
    .unwrap_err()
    .to_string();
    assert!(error.contains("IRONFLOW_MAX_PDF_MERGE_BYTES"), "{error}");
    assert_eq!(std::fs::read(&output).unwrap(), b"original");
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
}

#[tokio::test]
async fn byte_and_object_limits_are_enforced_independently() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("source.pdf");
    shared_resource_pdf(&source);
    let bytes = std::fs::metadata(&source).unwrap().len();

    let byte_request = Request {
        sources: vec![FileSource::path(source.clone())],
        output_path: directory.path().join("bytes.pdf"),
        limits: limits(100, 2, bytes.saturating_sub(1)),
    };
    let byte_error = run_tracked_blocking_step(move |execution| merge(byte_request, &execution))
        .await
        .unwrap_err()
        .to_string();
    assert!(
        byte_error.contains("IRONFLOW_MAX_PDF_BYTES"),
        "{byte_error}"
    );

    let total_request = Request {
        sources: vec![
            FileSource::path(source.clone()),
            FileSource::path(source.clone()),
        ],
        output_path: directory.path().join("total.pdf"),
        limits: Limits {
            files: 2,
            per_file_bytes: bytes,
            total_bytes: bytes.saturating_add(1),
            pages: 4,
            objects: 100,
        },
    };
    let total_error = run_tracked_blocking_step(move |execution| merge(total_request, &execution))
        .await
        .unwrap_err()
        .to_string();
    assert!(
        total_error.contains("IRONFLOW_MAX_PDF_MERGE_BYTES"),
        "{total_error}"
    );

    let object_request = Request {
        sources: vec![FileSource::path(source)],
        output_path: directory.path().join("objects.pdf"),
        limits: limits(7, 2, bytes.saturating_mul(2)),
    };
    let object_error =
        run_tracked_blocking_step(move |execution| merge(object_request, &execution))
            .await
            .unwrap_err()
            .to_string();
    assert!(
        object_error.contains("IRONFLOW_MAX_PDF_MERGE_OBJECTS"),
        "{object_error}"
    );
}
