#[path = "pdf_groups/fixture.rs"]
mod fixture;

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;
use lopdf::{Document, Object, dictionary};
use serde_json::json;

async fn split(
    source: &std::path::Path,
    output: &std::path::Path,
    pages: &str,
) -> anyhow::Result<ironflow::engine::types::NodeOutput> {
    NodeRegistry::with_builtins()
        .get("pdf_split")
        .unwrap()
        .execute(
            &json!({"path": source, "output_dir": output, "pages": pages, "pages_per_file": 2}),
            &Context::new(),
        )
        .await
}

#[tokio::test]
async fn late_collision_keeps_completed_group_and_existing_file_without_staging_debris() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("source.pdf");
    fixture::document(5).save(&source).unwrap();
    let output = directory.path().join("parts");
    std::fs::create_dir(&output).unwrap();
    std::fs::write(output.join("source_part_002.pdf"), b"existing").unwrap();
    let error = split(&source, &output, "all")
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("already exists"), "{error}");
    assert_eq!(
        Document::load(output.join("source_part_001.pdf"))
            .unwrap()
            .get_pages()
            .len(),
        2
    );
    assert_eq!(
        std::fs::read(output.join("source_part_002.pdf")).unwrap(),
        b"existing"
    );
    assert_eq!(std::fs::read_dir(&output).unwrap().count(), 2);
    assert!(!output.join("source_part_003.pdf").exists());
}

#[tokio::test]
async fn malformed_later_group_keeps_only_previously_completed_output() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("source.pdf");
    let mut document = fixture::document(5);
    let third = document.get_pages()[&3];
    document
        .get_dictionary_mut(third)
        .unwrap()
        .set("Parent", (99999, 0));
    document.save(&source).unwrap();
    let output = directory.path().join("parts");
    let error = format!("{:#}", split(&source, &output, "all").await.unwrap_err());
    assert!(error.contains("page-tree"), "{error}");
    assert_eq!(std::fs::read_dir(&output).unwrap().count(), 1);
    assert_eq!(
        Document::load(output.join("source_part_001.pdf"))
            .unwrap()
            .get_pages()
            .len(),
        2
    );
}

#[tokio::test]
async fn grouped_annotations_remap_retained_targets_and_disconnect_omitted_pages() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("source.pdf");
    let mut document = fixture::document(3);
    let pages = document.get_pages();
    let annotations = [pages[&2], pages[&3]].map(|target| {
        Object::Reference(document.add_object(dictionary! {
            "Type" => "Annot", "Subtype" => "Link", "P" => pages[&1],
            "Rect" => vec![0.into(), 0.into(), 10.into(), 10.into()],
            "Dest" => vec![Object::Reference(target), Object::Name(b"Fit".to_vec())],
        }))
    });
    document
        .get_dictionary_mut(pages[&1])
        .unwrap()
        .set("Annots", annotations.to_vec());
    document.save(&source).unwrap();
    let output = split(&source, &directory.path().join("parts"), "2,1")
        .await
        .unwrap();
    let grouped = Document::load(output["pdf_split_files"][0].as_str().unwrap()).unwrap();
    let pages = grouped.get_pages();
    let page = grouped.get_dictionary(pages[&2]).unwrap();
    let annotations = page.get(b"Annots").unwrap().as_array().unwrap();
    for (index, reference) in annotations.iter().enumerate() {
        let annotation = grouped.dereference(reference).unwrap().1.as_dict().unwrap();
        assert_eq!(annotation.get(b"P").unwrap(), &Object::Reference(pages[&2]));
        assert_eq!(
            annotation.get(b"Dest").unwrap().as_array().unwrap()[0],
            if index == 0 {
                Object::Reference(pages[&1])
            } else {
                Object::Null
            }
        );
    }
    assert_eq!(
        grouped
            .objects
            .values()
            .filter(|object| object.type_name().ok() == Some(b"Page".as_slice()))
            .count(),
        2
    );
    let bytes = std::fs::read(output["pdf_split_files"][0].as_str().unwrap()).unwrap();
    assert!(!String::from_utf8_lossy(&bytes).contains("Page 3"));
}

#[tokio::test]
async fn expired_deadline_prevents_grouped_output_directory_creation() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("source.pdf");
    fixture::document(5).save(&source).unwrap();
    let output = directory.path().join("parts");
    let error = ironflow::util::execution::with_execution_deadline(
        Some(tokio::time::Instant::now()),
        split(&source, &output, "all"),
    )
    .await
    .unwrap_err()
    .to_string();
    assert!(error.contains("deadline"), "{error}");
    assert!(!output.exists());
}

#[tokio::test]
async fn directory_destination_is_not_replaced() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("source.pdf");
    fixture::document(2).save(&source).unwrap();
    let output = directory.path().join("parts");
    let collision = output.join("source_part_001.pdf");
    std::fs::create_dir_all(&collision).unwrap();
    assert!(split(&source, &output, "all").await.is_err());
    assert!(collision.is_dir());
    assert_eq!(std::fs::read_dir(output).unwrap().count(), 1);
}

#[cfg(unix)]
#[tokio::test]
async fn final_symlinks_are_refused_but_directory_aliases_work() {
    use std::os::unix::fs::symlink;
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("source.pdf");
    fixture::document(2).save(&source).unwrap();
    let target = directory.path().join("target");
    for dangling in [true, false] {
        if !dangling {
            std::fs::write(&target, b"unchanged").unwrap();
        }
        let output = directory.path().join(dangling.to_string());
        std::fs::create_dir(&output).unwrap();
        symlink(&target, output.join("source_part_001.pdf")).unwrap();
        let error = split(&source, &output, "all")
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("symlink"), "{error}");
        assert_eq!(std::fs::read_dir(output).unwrap().count(), 1);
        if dangling {
            assert!(!target.exists());
        } else {
            assert_eq!(std::fs::read(&target).unwrap(), b"unchanged");
        }
    }
    let real = directory.path().join("real");
    std::fs::create_dir(&real).unwrap();
    let alias = directory.path().join("alias");
    symlink(&real, &alias).unwrap();
    split(&source, &alias, "all").await.unwrap();
    assert_eq!(
        Document::load(real.join("source_part_001.pdf"))
            .unwrap()
            .get_pages()
            .len(),
        2
    );
}
