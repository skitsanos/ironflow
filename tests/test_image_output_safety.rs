//! IF-179: the image nodes and `image_to_pdf` publish their output through the
//! rooted, no-follow, atomic writer. The nodes run in-process through the
//! registry, so every test drives the whole node path on scratch data.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use ironflow::engine::types::{Context, NodeOutput};
use ironflow::nodes::NodeRegistry;
use serde_json::{Value, json};

const ORIGINAL: &[u8] = b"previous output";

/// One node with the parameters it needs besides its source and destination.
#[derive(Clone)]
struct Case {
    node: &'static str,
    extension: &'static str,
    extra: Value,
}

fn case(node: &'static str, extension: &'static str, extra: Value) -> Case {
    Case {
        node,
        extension,
        extra,
    }
}

/// The six nodes that share one image writer.
fn shared_writer() -> Vec<Case> {
    vec![
        case("image_resize", "png", json!({"width": 3, "height": 2})),
        case(
            "image_crop",
            "png",
            json!({"x": 1, "y": 1, "width": 3, "height": 2}),
        ),
        case("image_rotate", "png", json!({"angle": 90})),
        case("image_flip", "png", json!({})),
        case("image_grayscale", "png", json!({})),
        case("image_watermark", "png", json!({"text": "IF"})),
    ]
}

fn convert() -> Vec<Case> {
    vec![
        case("image_convert", "png", json!({})),
        case("image_convert", "jpg", json!({"quality": 70})),
    ]
}

fn every_family() -> Vec<Case> {
    let mut cases = shared_writer();
    cases.extend(convert());
    cases.push(case("image_to_pdf", "pdf", json!({})));
    cases
}

fn config(case: &Case, source: &Path, destination: &Path) -> Value {
    let mut config = case.extra.clone();
    let fields = config.as_object_mut().unwrap();
    if case.node == "image_to_pdf" {
        fields.entry("sources").or_insert_with(|| json!([source]));
    } else {
        fields.insert("path".to_owned(), json!(source));
    }
    fields.insert("output_path".to_owned(), json!(destination));
    config
}

async fn execute(case: &Case, source: &Path, destination: &Path) -> anyhow::Result<NodeOutput> {
    NodeRegistry::with_builtins()
        .get(case.node)
        .unwrap()
        .execute(&config(case, source, destination), &Context::new())
        .await
}

fn scratch() -> (tempfile::TempDir, PathBuf) {
    let directory = tempfile::tempdir().unwrap();
    let work = directory.path().join("work");
    std::fs::create_dir(&work).unwrap();
    (directory, work)
}

fn staged_leftovers(work: &Path) -> usize {
    std::fs::read_dir(work)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with(".ironflow-")
        })
        .count()
}

fn gradient() -> image::RgbImage {
    image::RgbImage::from_fn(6, 4, |x, y| {
        image::Rgb([(x * 40) as u8, (y * 60) as u8, (x * y * 10) as u8])
    })
}

fn small_source(directory: &Path) -> PathBuf {
    let path = directory.join("source.png");
    gradient().save(&path).unwrap();
    path
}

/// A float-pixel source: the PNG encoder rejects `Rgb32F`, so encoding fails
/// only after the destination has been opened for writing.
fn float_source(directory: &Path) -> PathBuf {
    let path = directory.join("float.hdr");
    image::Rgb32FImage::from_pixel(6, 4, image::Rgb([0.5, 0.25, 1.0]))
        .save(&path)
        .unwrap();
    path
}

/// Incompressible pixels, written as a raw PPM so building the fixture costs
/// no encoder time while encoding it takes long enough to be interrupted.
fn noise_source(directory: &Path, side: u32) -> PathBuf {
    let path = directory.join("noise.ppm");
    let mut bytes = format!("P6\n{side} {side}\n255\n").into_bytes();
    let mut state = 0x2545_f491_4f6c_dd1d_u64;
    bytes.extend((0..u64::from(side) * u64::from(side) * 3).map(|_| {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (state >> 56) as u8
    }));
    std::fs::write(&path, bytes).unwrap();
    path
}

/// A small JPEG padded with comment segments. `image_to_pdf` embeds JPEG
/// bytes as they are, so twenty of these make a PDF large enough to cancel
/// while it is being written.
#[cfg(unix)]
fn padded_jpeg(directory: &Path) -> PathBuf {
    let plain = directory.join("plain.jpg");
    gradient().save(&plain).unwrap();
    let encoded = std::fs::read(&plain).unwrap();
    let mut padded = encoded[..2].to_vec();
    for _ in 0..30 {
        padded.extend([0xff, 0xfe, 0xff, 0xff]);
        padded.extend(std::iter::repeat_n(b'.', 0xffff - 2));
    }
    padded.extend(&encoded[2..]);
    let path = directory.join("padded.jpg");
    std::fs::write(&path, padded).unwrap();
    path
}

#[cfg(unix)]
#[tokio::test]
async fn every_family_refuses_a_destination_symlink_and_leaves_its_target_untouched() {
    use std::os::unix::fs::symlink;

    for case in every_family() {
        for target_exists in [true, false] {
            let label = format!("{} .{} existing={target_exists}", case.node, case.extension);
            let (directory, work) = scratch();
            let source = small_source(directory.path());
            let target = work.join("target.bin");
            let destination = work.join(format!("output.{}", case.extension));
            if target_exists {
                std::fs::write(&target, ORIGINAL).unwrap();
            }
            symlink("target.bin", &destination).unwrap();

            let error = execute(&case, &source, &destination)
                .await
                .expect_err(&label);

            let error = format!("{error:#}");
            assert!(error.contains("symlink"), "{label}: {error}");
            assert!(error.contains(case.node), "{label}: {error}");
            if target_exists {
                assert_eq!(std::fs::read(&target).unwrap(), ORIGINAL, "{label}");
            } else {
                assert!(!target.exists(), "{label}: a dangling link was followed");
            }
            assert!(
                std::fs::symlink_metadata(&destination)
                    .unwrap()
                    .file_type()
                    .is_symlink(),
                "{label}: the link itself must survive"
            );
            assert_eq!(staged_leftovers(&work), 0, "{label}");
        }
    }
}

#[cfg(unix)]
#[tokio::test]
async fn every_family_refuses_a_fifo_destination_without_waiting_for_a_reader() {
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::OpenOptionsExt;

    for case in every_family() {
        let label = format!("{} .{}", case.node, case.extension);
        let (directory, work) = scratch();
        let source = small_source(directory.path());
        let destination = work.join(format!("output.{}", case.extension));
        let name = std::ffi::CString::new(destination.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);

        let outcome = tokio::time::timeout(
            Duration::from_secs(5),
            execute(&case, &source, &destination),
        )
        .await;
        // A writer blocked in open(2) is released by this reader, so a
        // regression fails here instead of hanging the runtime on shutdown.
        let _reader = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(&destination)
            .unwrap();

        let error = outcome
            .unwrap_or_else(|_| panic!("{label}: the write waited for a FIFO reader"))
            .expect_err(&label);
        let error = format!("{error:#}");
        assert!(error.contains("regular file"), "{label}: {error}");
        assert_eq!(staged_leftovers(&work), 0, "{label}");
    }
}

#[tokio::test]
async fn encoder_failure_keeps_the_existing_destination_and_leaves_no_staging_file() {
    // `image_watermark` converts to RGBA before encoding and `image_to_pdf`
    // has no encoder that can fail after the save began, so neither has a
    // constructible encoder failure.
    let mut cases = shared_writer();
    cases.retain(|case| case.node != "image_watermark");
    cases.push(case("image_convert", "png", json!({})));

    for case in cases {
        let (directory, work) = scratch();
        let source = float_source(directory.path());
        let destination = work.join("output.png");
        std::fs::write(&destination, ORIGINAL).unwrap();

        let error = execute(&case, &source, &destination)
            .await
            .expect_err(case.node);

        let error = format!("{error:#}");
        assert!(error.contains("Rgb32F"), "{}: {error}", case.node);
        assert_eq!(
            std::fs::read(&destination).unwrap(),
            ORIGINAL,
            "{}: a failed encode must not replace the destination",
            case.node
        );
        assert_eq!(
            std::fs::read_dir(&work).unwrap().count(),
            1,
            "{}",
            case.node
        );
    }
}

/// Outcome of one attempt to cancel a node while it writes its output.
#[cfg(unix)]
enum Cancelled {
    /// The step was dropped after it had begun writing.
    DuringWrite,
    /// The step finished before the cancellation landed.
    TooLate,
}

/// Run `case` and drop it as soon as it starts writing: a staging file beside
/// the destination, or the destination's own inode changing length in place.
/// An atomically replaced destination has a new inode and is not a signal.
#[cfg(unix)]
async fn cancel_once_writing(case: &Case, source: &Path, destination: &Path) -> Cancelled {
    use std::os::unix::fs::MetadataExt;

    let node = NodeRegistry::with_builtins().get(case.node).unwrap();
    let config = config(case, source, destination);
    let inode = std::fs::metadata(destination).unwrap().ino();
    let task = tokio::spawn(async move { node.execute(&config, &Context::new()).await });
    let abort = task.abort_handle();
    let work = destination.parent().unwrap().to_owned();
    let watched = destination.to_owned();
    let watcher = std::thread::spawn(move || {
        let give_up = Instant::now() + Duration::from_secs(60);
        while Instant::now() < give_up && !abort.is_finished() {
            let Ok(metadata) = std::fs::metadata(&watched) else {
                return;
            };
            if metadata.ino() != inode {
                return;
            }
            if staged_leftovers(&work) > 0 || metadata.len() != ORIGINAL.len() as u64 {
                abort.abort();
                return;
            }
            std::thread::sleep(Duration::from_micros(100));
        }
    });
    let outcome = task.await;
    watcher.join().unwrap();
    match outcome {
        Err(error) if error.is_cancelled() => Cancelled::DuringWrite,
        other => {
            other.unwrap().unwrap();
            Cancelled::TooLate
        }
    }
}

/// Contract: a step that was cancelled while writing leaves the previous
/// destination and no staging file. A step that finished first is retried.
#[cfg(unix)]
async fn assert_cancellation_keeps_the_destination(case: Case, source: PathBuf, work: PathBuf) {
    let label = format!("{} .{}", case.node, case.extension);
    let destination = work.join(format!("output.{}", case.extension));
    for _attempt in 0..5 {
        std::fs::write(&destination, ORIGINAL).unwrap();
        if matches!(
            cancel_once_writing(&case, &source, &destination).await,
            Cancelled::TooLate
        ) {
            continue;
        }
        // The cancelled worker removes its staging file before it stops.
        let cleaned = Instant::now() + Duration::from_secs(30);
        while staged_leftovers(&work) > 0 && Instant::now() < cleaned {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert_eq!(
            std::fs::read(&destination).unwrap(),
            ORIGINAL,
            "{label}: a cancelled step must not replace or truncate the destination"
        );
        assert_eq!(std::fs::read_dir(&work).unwrap().count(), 1, "{label}");
        return;
    }
    panic!("{label}: the step never stopped while it was writing");
}

#[cfg(unix)]
#[tokio::test]
async fn cancelling_the_shared_image_writer_mid_encode_keeps_the_existing_destination() {
    let (directory, work) = scratch();
    let source = noise_source(directory.path(), 900);
    let crop = case(
        "image_crop",
        "png",
        json!({"x": 0, "y": 0, "width": 900, "height": 900}),
    );
    assert_cancellation_keeps_the_destination(crop, source, work).await;
}

#[cfg(unix)]
#[tokio::test]
async fn cancelling_image_convert_mid_encode_keeps_the_existing_destination() {
    for convert in convert() {
        let (directory, work) = scratch();
        let source = noise_source(directory.path(), 900);
        assert_cancellation_keeps_the_destination(convert, source, work).await;
    }
}

#[cfg(unix)]
#[tokio::test]
async fn cancelling_image_to_pdf_mid_save_keeps_the_existing_destination() {
    let (directory, work) = scratch();
    let source = padded_jpeg(directory.path());
    let pdf = case(
        "image_to_pdf",
        "pdf",
        json!({"sources": vec![json!(source); 20]}),
    );
    assert_cancellation_keeps_the_destination(pdf, source, work).await;
}

#[tokio::test]
async fn shared_writer_output_is_byte_for_byte_a_direct_encode() {
    use image::imageops::FilterType;

    for case in shared_writer() {
        let (directory, work) = scratch();
        let source = small_source(directory.path());
        let destination = work.join("output.png");
        std::fs::write(&destination, ORIGINAL).unwrap();

        execute(&case, &source, &destination).await.unwrap();

        let written = std::fs::read(&destination).unwrap();
        let decoded = image::open(&source).unwrap();
        let expected = match case.node {
            "image_resize" => decoded.resize_exact(3, 2, FilterType::Lanczos3),
            "image_crop" => decoded.crop_imm(1, 1, 3, 2),
            "image_rotate" => decoded.rotate90(),
            "image_flip" => decoded.fliph(),
            "image_grayscale" => decoded.grayscale(),
            // The watermark band is drawn by the node; its pixels are
            // re-encoded here the way the node always encoded them.
            _ => image::load_from_memory(&written).unwrap(),
        };
        let direct = directory.path().join("direct.png");
        expected
            .save_with_format(&direct, image::ImageFormat::Png)
            .unwrap();
        assert_eq!(written, std::fs::read(&direct).unwrap(), "{}", case.node);
        assert_eq!(
            std::fs::read_dir(&work).unwrap().count(),
            1,
            "{}",
            case.node
        );
    }
}

#[tokio::test]
async fn image_convert_output_is_byte_for_byte_a_direct_encode() {
    let (directory, work) = scratch();
    let source = small_source(directory.path());
    let decoded = image::open(&source).unwrap();

    let png = work.join("output.png");
    execute(&convert()[0], &source, &png).await.unwrap();
    let direct = directory.path().join("direct.png");
    decoded
        .save_with_format(&direct, image::ImageFormat::Png)
        .unwrap();
    assert_eq!(
        std::fs::read(&png).unwrap(),
        std::fs::read(&direct).unwrap()
    );

    let jpeg = work.join("output.jpg");
    std::fs::write(&jpeg, ORIGINAL).unwrap();
    execute(&convert()[1], &source, &jpeg).await.unwrap();
    let mut direct = Vec::new();
    decoded
        .to_rgb8()
        .write_with_encoder(image::codecs::jpeg::JpegEncoder::new_with_quality(
            &mut direct,
            70,
        ))
        .unwrap();
    assert_eq!(std::fs::read(&jpeg).unwrap(), direct);
    assert_eq!(staged_leftovers(&work), 0);
}

#[tokio::test]
async fn a_missing_parent_directory_is_still_an_error_and_nothing_is_created() {
    for case in every_family() {
        let (directory, work) = scratch();
        let source = small_source(directory.path());
        let destination = work
            .join("missing/deeper")
            .join(format!("output.{}", case.extension));

        execute(&case, &source, &destination)
            .await
            .expect_err(case.node);

        assert!(!work.join("missing").exists(), "{}", case.node);
        assert_eq!(
            std::fs::read_dir(&work).unwrap().count(),
            0,
            "{}",
            case.node
        );
    }
}

#[tokio::test]
async fn an_existing_regular_destination_is_replaced_without_a_staging_leftover() {
    for case in every_family() {
        let (directory, work) = scratch();
        let source = small_source(directory.path());
        let destination = work.join(format!("output.{}", case.extension));
        std::fs::write(&destination, ORIGINAL).unwrap();

        execute(&case, &source, &destination).await.unwrap();

        let written = std::fs::read(&destination).unwrap();
        let magic: &[u8] = match case.extension {
            "png" => b"\x89PNG",
            "jpg" => b"\xff\xd8",
            _ => b"%PDF",
        };
        assert!(written.starts_with(magic), "{}", case.node);
        assert_eq!(
            std::fs::read_dir(&work).unwrap().count(),
            1,
            "{}",
            case.node
        );
    }
}
