use std::io::{Cursor, Read, Seek, SeekFrom, Write};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, mpsc};

use super::{
    central_offset, check_bytes, check_xlsx, check_xlsx_bytes, eocd_offset, header_len,
    standard_zip,
};

#[test]
fn cumulative_names_extra_fields_and_comments_have_an_xlsx_ceiling() {
    let mut cursor = Cursor::new(Vec::new());
    {
        let mut writer = zip::ZipWriter::new(&mut cursor);
        let mut options = zip::write::SimpleFileOptions::default()
            .into_full_options()
            .with_file_comment("c".repeat(2_048));
        options
            .add_extra_data(0xBEEF, vec![b'x'; 4_096], true)
            .unwrap();
        writer.start_file("n".repeat(1_024), options).unwrap();
        writer.write_all(b"x").unwrap();
        writer.finish().unwrap();
    }
    let bytes = cursor.into_inner();
    let central = central_offset(&bytes);
    let metadata_bytes = (header_len(&bytes, central) - 46) as u64;

    check_xlsx_bytes(bytes.clone(), 1, bytes.len() as u64, metadata_bytes).unwrap();
    let error = check_xlsx_bytes(bytes, 1, u64::MAX, metadata_bytes - 1)
        .unwrap_err()
        .to_string();

    assert!(
        error.contains("IRONFLOW_MAX_XLSX_ARCHIVE_METADATA_BYTES"),
        "{error}"
    );
    assert!(error.contains(&metadata_bytes.to_string()), "{error}");
}

#[test]
fn every_central_header_is_validated() {
    let mut bytes = standard_zip(2, b"");
    let first = central_offset(&bytes);
    let second = first + header_len(&bytes, first);
    bytes[second..second + 4].copy_from_slice(b"NOPE");

    let error = check_bytes(bytes, 2, 1024 * 1024).unwrap_err().to_string();
    assert!(error.contains("entry 1 has an invalid header"), "{error}");
}

#[test]
fn central_headers_must_exactly_fill_the_declared_directory() {
    let mut bytes = standard_zip(1, b"");
    let old_eocd = eocd_offset(&bytes);
    let old_size = u32::from_le_bytes(bytes[old_eocd + 12..old_eocd + 16].try_into().unwrap());
    bytes.insert(old_eocd, 0);
    let new_eocd = old_eocd + 1;
    bytes[new_eocd + 12..new_eocd + 16].copy_from_slice(&(old_size + 1).to_le_bytes());

    let error = check_bytes(bytes, 1, 1024 * 1024).unwrap_err().to_string();
    assert!(error.contains("do not exactly fill"), "{error}");
}

#[test]
fn central_header_lengths_cannot_escape_the_directory() {
    let mut bytes = standard_zip(1, b"");
    let central = central_offset(&bytes);
    bytes[central + 28..central + 30].copy_from_slice(&u16::MAX.to_le_bytes());

    let error = check_bytes(bytes, 1, 1024 * 1024).unwrap_err().to_string();
    assert!(
        error.contains("extends past its declared bounds"),
        "{error}"
    );
}

struct SlowCentralReader {
    inner: Cursor<Vec<u8>>,
    central_start: u64,
    central_end: u64,
    reads: Arc<AtomicUsize>,
    started: Option<mpsc::Sender<()>>,
}

impl Read for SlowCentralReader {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let position = self.inner.position();
        if buffer.len() == 46 && position >= self.central_start && position < self.central_end {
            self.reads.fetch_add(1, Ordering::SeqCst);
            if let Some(started) = self.started.take() {
                let _ = started.send(());
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        self.inner.read(buffer)
    }
}

impl Seek for SlowCentralReader {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        self.inner.seek(position)
    }
}

#[tokio::test]
async fn cancellation_is_checked_between_central_headers() {
    const TOTAL: usize = 1_000;
    let bytes = standard_zip(TOTAL, b"");
    let central_start = central_offset(&bytes) as u64;
    let central_end = eocd_offset(&bytes) as u64;
    let reads = Arc::new(AtomicUsize::new(0));
    let worker_reads = reads.clone();
    let (started_tx, started_rx) = mpsc::channel();
    let (finished_tx, finished_rx) = mpsc::channel();

    let waiter = tokio::spawn(crate::util::execution::run_blocking_step(
        move |execution| {
            let mut reader = SlowCentralReader {
                inner: Cursor::new(bytes),
                central_start,
                central_end,
                reads: worker_reads,
                started: Some(started_tx),
            };
            let result = check_xlsx(
                &mut reader,
                std::path::Path::new("fixture.xlsx"),
                TOTAL as u64,
                1024 * 1024,
                1024 * 1024,
                Some(&execution),
            );
            let _ = finished_tx.send(result.as_ref().err().map(ToString::to_string));
            result
        },
    ));

    tokio::task::spawn_blocking(move || started_rx.recv())
        .await
        .unwrap()
        .unwrap();
    waiter.abort();
    assert!(waiter.await.unwrap_err().is_cancelled());
    let error = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        tokio::task::spawn_blocking(move || finished_rx.recv()),
    )
    .await
    .expect("central-directory worker ignored cancellation")
    .unwrap()
    .unwrap()
    .expect("central-directory worker unexpectedly reached EOF");

    assert!(error.contains("step execution cancelled"), "{error}");
    assert!(reads.load(Ordering::SeqCst) < TOTAL);
}
