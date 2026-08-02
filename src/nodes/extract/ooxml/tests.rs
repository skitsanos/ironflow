use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use super::*;

fn write_archive(path: &Path, entries: &[(&str, &[u8], Option<u32>)]) {
    let file = File::create(path).unwrap();
    let mut writer = zip::ZipWriter::new(file);
    for (name, body, mode) in entries {
        let mut options = zip::write::SimpleFileOptions::default();
        if let Some(mode) = mode {
            options = options.unix_permissions(*mode);
        }
        writer.start_file(*name, options).unwrap();
        writer.write_all(body).unwrap();
    }
    writer.finish().unwrap();
}

fn patch_declared_size(path: &Path, size: u32) {
    let mut bytes = std::fs::read(path).unwrap();
    let local = bytes
        .windows(4)
        .position(|window| window == b"PK\x03\x04")
        .unwrap();
    bytes[local + 22..local + 26].copy_from_slice(&size.to_le_bytes());
    let central = bytes
        .windows(4)
        .position(|window| window == b"PK\x01\x02")
        .unwrap();
    bytes[central + 24..central + 28].copy_from_slice(&size.to_le_bytes());
    std::fs::write(path, bytes).unwrap();
}

fn patch_crc(path: &Path, crc: u32) {
    let mut bytes = std::fs::read(path).unwrap();
    let local = bytes
        .windows(4)
        .position(|window| window == b"PK\x03\x04")
        .unwrap();
    bytes[local + 14..local + 18].copy_from_slice(&crc.to_le_bytes());
    let central = bytes
        .windows(4)
        .position(|window| window == b"PK\x01\x02")
        .unwrap();
    bytes[central + 16..central + 20].copy_from_slice(&crc.to_le_bytes());
    std::fs::write(path, bytes).unwrap();
}

fn limits(max_entries: u64, max_zip_bytes: u64) -> Limits {
    Limits {
        max_output_bytes: 1024,
        max_items: 1024,
        max_zip_entries: max_entries,
        max_zip_bytes,
        max_pdf_pages: 10,
    }
}

#[tokio::test]
async fn declared_and_actual_archive_budgets_are_cumulative() {
    let directory = tempfile::tempdir().unwrap();
    let declared_path = directory.path().join("declared.docx");
    write_archive(&declared_path, &[("word/document.xml", b"12345", None)]);
    let error = crate::util::execution::run_blocking_step(move |execution| {
        Archive::open(&declared_path, "extract_word", limits(10, 4), &execution).map(|_| ())
    })
    .await
    .unwrap_err()
    .to_string();
    assert!(error.contains("declared uncompressed"), "{error}");

    let actual_path = directory.path().join("actual.docx");
    write_archive(&actual_path, &[("word/document.xml", b"123456", None)]);
    let error = crate::util::execution::run_blocking_step(move |execution| {
        let mut archive = Archive::open(&actual_path, "extract_word", limits(10, 10), &execution)?;
        archive.with_required_xml("word/document.xml", &execution, |_| Ok(()))?;
        archive
            .with_required_xml("word/document.xml", &execution, |_| Ok(()))
            .map(|_| ())
    })
    .await
    .unwrap_err()
    .to_string();
    assert!(error.contains("cumulative or per-part"), "{error}");

    let understated_path = directory.path().join("understated.docx");
    write_archive(
        &understated_path,
        &[("word/document.xml", b"1234567890", None)],
    );
    patch_declared_size(&understated_path, 1);
    let error = crate::util::execution::run_blocking_step(move |execution| {
        let mut archive =
            Archive::open(&understated_path, "extract_word", limits(10, 5), &execution)?;
        archive
            .with_required_xml("word/document.xml", &execution, |_| Ok(()))
            .map(|_| ())
    })
    .await
    .unwrap_err()
    .to_string();
    assert!(error.contains("cumulative or per-part"), "{error}");
}

#[tokio::test]
async fn streamed_parts_validate_utf8_crc_and_complete_the_entry() {
    let directory = tempfile::tempdir().unwrap();
    let partial_path = directory.path().join("partial.docx");
    write_archive(
        &partial_path,
        &[("word/document.xml", b"<root>payload</root>", None)],
    );
    crate::util::execution::run_blocking_step(move |execution| {
        let mut archive =
            Archive::open(&partial_path, "extract_word", limits(10, 100), &execution)?;
        archive.with_required_xml("word/document.xml", &execution, |reader| {
            let mut byte = [0_u8; 1];
            reader.read_exact(&mut byte)?;
            Ok(())
        })?;
        assert_eq!(archive.actual_bytes, 20);
        Ok(())
    })
    .await
    .unwrap();

    let utf8_path = directory.path().join("utf8.docx");
    write_archive(
        &utf8_path,
        &[("word/document.xml", b"<root>\xff</root>", None)],
    );
    let error = crate::util::execution::run_blocking_step(move |execution| {
        let mut archive = Archive::open(&utf8_path, "extract_word", limits(10, 100), &execution)?;
        archive
            .with_required_xml("word/document.xml", &execution, |_| Ok(()))
            .map(|_| ())
    })
    .await
    .unwrap_err()
    .to_string();
    assert!(error.contains("not UTF-8"), "{error}");

    let crc_path = directory.path().join("crc.docx");
    write_archive(&crc_path, &[("word/document.xml", b"<root/>", None)]);
    patch_crc(&crc_path, 0x1234_5678);
    let error = crate::util::execution::run_blocking_step(move |execution| {
        let mut archive = Archive::open(&crc_path, "extract_word", limits(10, 100), &execution)?;
        archive
            .with_required_xml("word/document.xml", &execution, |_| Ok(()))
            .map(|_| ())
    })
    .await
    .unwrap_err()
    .to_string();
    assert!(error.contains("Invalid checksum"), "{error}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn streamed_parts_checkpoint_after_cancellation() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("cancel.docx");
    let xml = format!("<root>{}</root>", "x".repeat(512));
    write_archive(&path, &[("word/document.xml", xml.as_bytes(), None)]);
    let started = Arc::new(AtomicBool::new(false));
    let release = Arc::new(AtomicBool::new(false));
    let finished = Arc::new(AtomicBool::new(false));
    let message = Arc::new(std::sync::Mutex::new(None));
    let worker_started = Arc::clone(&started);
    let worker_release = Arc::clone(&release);
    let worker_finished = Arc::clone(&finished);
    let worker_message = Arc::clone(&message);

    let task = tokio::spawn(async move {
        crate::util::execution::run_blocking_step(move |execution| {
            let result = (|| {
                let mut archive =
                    Archive::open(&path, "extract_word", limits(10, 1000), &execution)?;
                archive.with_required_xml("word/document.xml", &execution, |reader| {
                    let mut first = [0_u8; 1];
                    reader.read_exact(&mut first)?;
                    worker_started.store(true, Ordering::Release);
                    while !worker_release.load(Ordering::Acquire) {
                        std::thread::sleep(Duration::from_millis(2));
                    }
                    std::io::copy(reader, &mut std::io::sink())?;
                    Ok(())
                })
            })();
            *worker_message.lock().unwrap() = result.as_ref().err().map(ToString::to_string);
            worker_finished.store(true, Ordering::Release);
            result
        })
        .await
    });

    wait_for_flag(&started).await;
    task.abort();
    let _ = task.await;
    release.store(true, Ordering::Release);
    wait_for_flag(&finished).await;
    let error = message.lock().unwrap().clone().unwrap();
    assert!(error.contains("cancelled"), "{error}");
}

async fn wait_for_flag(flag: &AtomicBool) {
    tokio::time::timeout(Duration::from_secs(2), async {
        while !flag.load(Ordering::Acquire) {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn entry_count_and_symlink_parts_are_rejected() {
    let directory = tempfile::tempdir().unwrap();
    let count_path = directory.path().join("count.docx");
    write_archive(
        &count_path,
        &[("one.xml", b"1", None), ("two.xml", b"2", None)],
    );
    let error = crate::util::execution::run_blocking_step(move |execution| {
        Archive::open(&count_path, "extract_word", limits(1, 100), &execution).map(|_| ())
    })
    .await
    .unwrap_err()
    .to_string();
    assert!(error.contains("IRONFLOW_MAX_ZIP_ENTRIES (1)"), "{error}");

    let symlink_path = directory.path().join("symlink.docx");
    let file = File::create(&symlink_path).unwrap();
    let mut writer = zip::ZipWriter::new(file);
    writer
        .add_symlink(
            "word/document.xml",
            "target",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
    writer.finish().unwrap();
    let error = crate::util::execution::run_blocking_step(move |execution| {
        Archive::open(&symlink_path, "extract_word", limits(10, 100), &execution).map(|_| ())
    })
    .await
    .unwrap_err()
    .to_string();
    assert!(error.contains("symlink parts"), "{error}");
}

#[tokio::test]
async fn duplicate_part_names_are_rejected_without_copying_all_names() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("duplicate.docx");
    write_archive(
        &path,
        &[("a.xml", b"first", None), ("b.xml", b"second", None)],
    );
    // `ZipWriter` rejects duplicates, so make the two equal-length local and
    // central names identical after constructing an otherwise valid archive.
    let mut bytes = std::fs::read(&path).unwrap();
    for index in 0..=bytes.len() - b"b.xml".len() {
        if &bytes[index..index + b"b.xml".len()] == b"b.xml" {
            bytes[index..index + b"a.xml".len()].copy_from_slice(b"a.xml");
        }
    }
    std::fs::write(&path, bytes).unwrap();

    let error = crate::util::execution::run_blocking_step(move |execution| {
        Archive::open(&path, "extract_word", limits(10, 100), &execution).map(|_| ())
    })
    .await
    .unwrap_err()
    .to_string();

    assert!(error.contains("duplicate archive part"), "{error}");
}
use std::path::Path;
