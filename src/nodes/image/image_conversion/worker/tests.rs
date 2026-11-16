use super::*;
use crate::util::execution::run_tracked_blocking_step;

fn two_page_document(execution: &ExecutionControl) -> Document {
    let mut document = Document::with_version("1.5");
    let pages_id = document.new_object_id();
    let mut page_ids = Vec::new();
    for (width, height) in [(3, 2), (640, 480)] {
        let pixels = vec![0x5a; width as usize * height as usize * 3];
        let image = Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Image",
                "Width" => i64::from(width),
                "Height" => i64::from(height),
                "ColorSpace" => "DeviceRGB",
                "BitsPerComponent" => 8,
            },
            pixels,
        );
        add_page(&mut document, pages_id, &mut page_ids, image, width, height).unwrap();
    }
    finish_document(&mut document, pages_id, &page_ids, execution).unwrap();
    document
}

#[tokio::test]
async fn staged_save_writes_the_bytes_of_a_plain_lopdf_save() {
    let directory = tempfile::tempdir().unwrap();
    let plain = directory.path().join("plain.pdf");
    let staged = directory.path().join("staged.pdf");
    std::fs::write(&staged, b"previous output").unwrap();
    let (plain_path, staged_path) = (plain.clone(), staged.clone());

    run_tracked_blocking_step(move |execution| {
        let mut document = two_page_document(&execution);
        document.clone().save(&plain_path)?;
        save(&mut document, staged_path.to_str().unwrap(), &execution)
    })
    .await
    .unwrap();

    let written = std::fs::read(&staged).unwrap();
    assert!(written.starts_with(b"%PDF-1.5"));
    assert_eq!(written, std::fs::read(&plain).unwrap());
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 2);
}
