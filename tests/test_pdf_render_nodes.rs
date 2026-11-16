//! `pdf_to_image` and `pdf_thumbnail` executed through the CLI with the
//! built-in renderer: no native library, no skips (IF-194).
#![cfg(feature = "pdf-render")]

#[path = "pdf_pages/hostile.rs"]
mod hostile;
// Only the encrypted fixture is used here.
#[allow(dead_code)]
#[path = "pdf_pages/object_stream.rs"]
mod object_stream;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use tokio::process::Command;

fn sample() -> Vec<u8> {
    std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/fixtures/ironflow-sample.pdf"),
    )
    .unwrap()
}

struct Run {
    success: bool,
    text: String,
    elapsed: Duration,
}

/// Run `steps` (Lua statements on `flow`) against `source.pdf` in `directory`.
async fn run(directory: &Path, pdf: &[u8], steps: &str, environment: &[(&str, &str)]) -> Run {
    std::fs::write(directory.join("source.pdf"), pdf).unwrap();
    std::fs::write(
        directory.join("flow.lua"),
        format!("local flow = Flow.new('pdf_render')\n{steps}\nreturn flow\n"),
    )
    .unwrap();
    let started = Instant::now();
    let output = tokio::time::timeout(
        Duration::from_secs(120),
        Command::new(env!("CARGO_BIN_EXE_ironflow"))
            .env_clear()
            .envs(environment.iter().copied())
            .env("IRONFLOW_ARTIFACT_DIR", directory.join("artifacts"))
            .current_dir(directory)
            .arg("run")
            .arg("flow.lua")
            .arg("--store-dir")
            .arg(directory.join("store"))
            .kill_on_drop(true)
            .output(),
    )
    .await
    .expect("PDF render CLI did not settle")
    .unwrap();
    Run {
        success: output.status.success(),
        text: format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ),
        elapsed: started.elapsed(),
    }
}

async fn rejected(pdf: &[u8], steps: &str, environment: &[(&str, &str)], expected: &str) -> Run {
    let directory = tempfile::tempdir().unwrap();
    let outcome = run(directory.path(), pdf, steps, environment).await;
    assert!(!outcome.success, "{}", outcome.text);
    assert!(
        outcome.text.contains(expected),
        "{expected}: {}",
        outcome.text
    );
    assert!(artifacts(directory.path()).is_empty(), "{}", outcome.text);
    outcome
}

/// Every stored artifact, decoded.
fn artifacts(directory: &Path) -> Vec<(image::ImageFormat, image::RgbaImage)> {
    fn files(directory: &Path, found: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(directory) else {
            return;
        };
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                files(&entry.path(), found);
            } else {
                found.push(entry.path());
            }
        }
    }
    let mut found = Vec::new();
    files(&directory.join("artifacts"), &mut found);
    found
        .iter()
        .filter_map(|path| {
            let bytes = std::fs::read(path).ok()?;
            let format = image::guess_format(&bytes).ok()?;
            let decoded = image::load_from_memory_with_format(&bytes, format).ok()?;
            Some((format, decoded.into_rgba8()))
        })
        .collect()
}

/// A rendered text page is mostly white paper with some dark ink.
fn assert_text_page(image: &image::RgbaImage) {
    let dark = image.pixels().filter(|pixel| pixel.0[0] < 96).count();
    let white = image
        .pixels()
        .filter(|pixel| pixel.0[..3] == [255; 3])
        .count();
    let total = (image.width() * image.height()) as usize;
    assert!(dark > total / 1000, "page has no ink: {dark} of {total}");
    assert!(white > total / 2, "page has no paper: {white} of {total}");
    assert!(image.pixels().all(|pixel| pixel.0[3] == 255));
}

#[tokio::test]
async fn the_same_document_renders_repeatedly_in_one_process() {
    let directory = tempfile::tempdir().unwrap();
    let outcome = run(
        directory.path(),
        &sample(),
        r#"
flow:step('first', nodes.pdf_to_image({ path = 'source.pdf', pages = '1', dpi = 72, output_key = 'first' }))
flow:step('second', nodes.pdf_to_image({ path = 'source.pdf', pages = '1', dpi = 72, output_key = 'second' })):depends_on('first')
flow:step('third', nodes.pdf_thumbnail({ path = 'source.pdf', page = 1, output_key = 'third' })):depends_on('second')
flow:step('check', function(ctx)
    assert(ctx.page_count == 3)
    assert(ctx.first[1].width == 612 and ctx.first[1].height == 792 and ctx.first[1].page == 1)
    assert(ctx.first[1].artifact.sha256 == ctx.second[1].artifact.sha256, 'renders differ')
    assert(ctx.third.width == 198 and ctx.third.height == 256 and ctx.third_count == 1)
    return {}
end):depends_on('third')
"#,
        &[],
    )
    .await;
    assert!(outcome.success, "{}", outcome.text);
    let stored = artifacts(directory.path());
    // Identical renders share one content-addressed artifact.
    assert_eq!(stored.len(), 2, "{}", outcome.text);
    for (format, image) in &stored {
        assert_eq!(*format, image::ImageFormat::Png);
        assert_text_page(image);
    }
    assert!(
        stored
            .iter()
            .any(|(_, image)| image.dimensions() == (612, 792))
    );
    assert!(
        stored
            .iter()
            .any(|(_, image)| image.dimensions() == (198, 256))
    );
}

#[tokio::test]
async fn page_ranges_formats_and_resolution_follow_the_request() {
    let directory = tempfile::tempdir().unwrap();
    let outcome = run(
        directory.path(),
        &sample(),
        r#"
flow:step('range', nodes.pdf_to_image({ path = 'source.pdf', pages = '1-2,3', dpi = 36, format = 'jpeg' }))
flow:step('all', nodes.pdf_to_image({ path = 'source.pdf', dpi = 144, output_key = 'all' })):depends_on('range')
flow:step('check', function(ctx)
    assert(#ctx.images == 3 and #ctx.all == 3)
    for index, image in ipairs(ctx.images) do
        assert(image.page == index and image.format == 'jpeg')
        assert(image.width == 306 and image.height == 396)
        assert(image.artifact.mime_type == 'image/jpeg')
    end
    for index, image in ipairs(ctx.all) do
        assert(image.page == index and image.format == 'png')
        assert(image.width == 1224 and image.height == 1584)
    end
    return {}
end):depends_on('all')
"#,
        &[],
    )
    .await;
    assert!(outcome.success, "{}", outcome.text);
    let stored = artifacts(directory.path());
    assert_eq!(stored.len(), 6, "{}", outcome.text);
    for (format, image) in &stored {
        match format {
            image::ImageFormat::Jpeg => assert_eq!(image.dimensions(), (306, 396)),
            _ => {
                assert_eq!(image.dimensions(), (1224, 1584));
                assert_text_page(image);
            }
        }
    }
}

#[tokio::test]
async fn thumbnails_honour_size_width_and_height() {
    let directory = tempfile::tempdir().unwrap();
    let outcome = run(
        directory.path(),
        &sample(),
        r#"
flow:step('side', nodes.pdf_thumbnail({ path = 'source.pdf', page = 2, size = 100, output_key = 'side' }))
flow:step('wide', nodes.pdf_thumbnail({ path = 'source.pdf', page = 3, width = 200, format = 'jpg', output_key = 'wide' })):depends_on('side')
flow:step('tall', nodes.pdf_thumbnail({ path = 'source.pdf', height = 99, output_key = 'tall' })):depends_on('wide')
flow:step('both', nodes.pdf_thumbnail({ path = 'source.pdf', width = 50, height = 40, output_key = 'both' })):depends_on('tall')
flow:step('check', function(ctx)
    assert(ctx.side.width == 77 and ctx.side.height == 100 and ctx.side.page == 2)
    assert(ctx.wide.width == 200 and ctx.wide.height == 259 and ctx.wide.format == 'jpeg')
    assert(ctx.tall.width == 77 and ctx.tall.height == 99 and ctx.tall.page == 1)
    assert(ctx.both.width == 50 and ctx.both.height == 40)
    return {}
end):depends_on('both')
"#,
        &[],
    )
    .await;
    assert!(outcome.success, "{}", outcome.text);
    let mut sizes: Vec<_> = artifacts(directory.path())
        .iter()
        .map(|(_, image)| image.dimensions())
        .collect();
    sizes.sort_unstable();
    assert_eq!(sizes, [(50, 40), (77, 99), (77, 100), (200, 259)]);
}

#[tokio::test]
async fn resolution_page_and_pixel_limits_are_refused_before_rendering() {
    let image = "flow:step('render', nodes.pdf_to_image({ path = 'source.pdf', dpi = 300 }))";
    rejected(
        &sample(),
        image,
        &[("IRONFLOW_MAX_PDF_DPI", "299")],
        "IRONFLOW_MAX_PDF_DPI",
    )
    .await;
    rejected(
        &sample(),
        image,
        &[("IRONFLOW_MAX_PDF_RENDER_PAGES", "2")],
        "IRONFLOW_MAX_PDF_RENDER_PAGES",
    )
    .await;
    // 2550 x 3300 at 300 DPI: one pixel under the page is refused.
    let pixels = [("IRONFLOW_MAX_PDF_RENDER_PIXELS", "8414999")];
    rejected(&sample(), image, &pixels, "IRONFLOW_MAX_PDF_RENDER_PIXELS").await;
    let thumbnail = "flow:step('render', nodes.pdf_thumbnail({ path = 'source.pdf', width = 70000, height = 400 }))";
    rejected(&sample(), thumbnail, &[], "IRONFLOW_MAX_PDF_RENDER_PIXELS").await;
    let pixels = [("IRONFLOW_MAX_PDF_RENDER_PIXELS", "30000000")];
    rejected(&sample(), thumbnail, &pixels, "maximum side of 65535").await;
    let beyond = "flow:step('render', nodes.pdf_thumbnail({ path = 'source.pdf', page = 4 }))";
    rejected(
        &sample(),
        beyond,
        &[],
        "page 4 exceeds document page count (3)",
    )
    .await;
}

#[tokio::test]
async fn hostile_pages_are_refused_before_they_are_drawn() {
    let render = "flow:step('render', nodes.pdf_to_image({ path = 'source.pdf', dpi = 72 }))";
    let operations = "IRONFLOW_MAX_PDF_RENDER_OPERATIONS";
    // 2^40 fills and 10^16 tile copies stop at the operation budget.
    let budget = [(operations, "20000")];
    for pdf in [hostile::exponential_forms(40), hostile::dense_tiling()] {
        let outcome = rejected(&pdf, render, &budget, operations).await;
        assert!(
            outcome.elapsed < Duration::from_secs(60),
            "{:?}",
            outcome.elapsed
        );
    }
    rejected(&sample(), render, &[(operations, "10")], operations).await;
    rejected(
        &hostile::declared_image(60_000),
        render,
        &[],
        "embeds a 60000x60000 image, exceeds IRONFLOW_MAX_IMAGE_PIXELS",
    )
    .await;
    let stream = "IRONFLOW_MAX_PDF_DECOMPRESSED_STREAM_BYTES";
    rejected(
        &hostile::content_bomb(1 << 20),
        render,
        &[(stream, "65536")],
        stream,
    )
    .await;

    // The same pages render once they fit their limits.
    for pdf in [hostile::exponential_forms(4), hostile::declared_image(8)] {
        let directory = tempfile::tempdir().unwrap();
        let outcome = run(directory.path(), &pdf, render, &[]).await;
        assert!(outcome.success, "{}", outcome.text);
        assert_eq!(artifacts(directory.path())[0].1.dimensions(), (200, 200));
    }
}

#[tokio::test]
async fn a_step_timeout_and_a_run_deadline_stop_a_page_being_admitted() {
    // Unlimited operations leave only cancellation to end 2^40 fills.
    let unlimited = ("IRONFLOW_MAX_PDF_RENDER_OPERATIONS", "18446744073709551615");
    let pdf = hostile::exponential_forms(40);
    let timed = "flow:step('render', nodes.pdf_to_image({ path = 'source.pdf' })):timeout(1)";
    let outcome = rejected(&pdf, timed, &[unlimited], "render").await;
    assert!(outcome.text.contains("timed out"), "{}", outcome.text);
    assert!(
        outcome.elapsed < Duration::from_secs(30),
        "{:?}",
        outcome.elapsed
    );

    let untimed = "flow:step('render', nodes.pdf_thumbnail({ path = 'source.pdf' }))";
    let deadline = [unlimited, ("IRONFLOW_MAX_RUN_SECONDS", "1")];
    let outcome = rejected(&pdf, untimed, &deadline, "cancelled").await;
    assert!(
        outcome.elapsed < Duration::from_secs(30),
        "{:?}",
        outcome.elapsed
    );
}

#[tokio::test]
async fn malformed_and_encrypted_documents_fail_with_typed_errors() {
    let sample = sample();
    for node in ["pdf_to_image", "pdf_thumbnail"] {
        let render = format!("flow:step('render', nodes.{node}({{ path = 'source.pdf' }}))");
        let parse = format!("{node}: failed to parse PDF");
        rejected(b"", &render, &[], &parse).await;
        rejected(b"%PDF-1.7\nnot a document", &render, &[], &parse).await;
        rejected(&sample[..sample.len() / 2], &render, &[], &parse).await;
        let encrypted = object_stream::encrypted(0, "fixture-password");
        rejected(
            &encrypted,
            &render,
            &[],
            &format!("{node}: PDF requires a password"),
        )
        .await;
        let oversized = [("IRONFLOW_MAX_PDF_BYTES", "1024")];
        rejected(&sample, &render, &oversized, "IRONFLOW_MAX_PDF_BYTES").await;
        let missing = format!("flow:step('render', nodes.{node}({{ path = 'absent.pdf' }}))");
        rejected(&sample, &missing, &[], "absent.pdf").await;
    }
}
