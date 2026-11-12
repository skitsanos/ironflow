//! IF-178: ToUnicode ranges and glyph strings are bounded before lopdf expands them.

#[path = "pdf_cmap/fixture.rs"]
mod fixture;
#[cfg(unix)]
#[path = "pdf_cmap/measured.rs"]
mod measured;

use fixture::{document, extract, long_target, range, range_pdf, show};

const CMAP_LIMIT: &str = "IRONFLOW_MAX_PDF_EXTRACT_CMAP_ENTRIES";
const OUTPUT_LIMIT: &str = "IRONFLOW_MAX_EXTRACT_OUTPUT_BYTES";
/// Deadline for a CLI run whose work is bounded on any build.
#[cfg(unix)]
const PATIENCE: std::time::Duration = std::time::Duration::from_secs(120);

fn narrow() -> lopdf::Document {
    range_pdf("0001", "00FF", vec![0, 1, 0, 2, 0, 3])
}

#[tokio::test]
async fn a_narrow_range_extracts_unchanged() {
    assert_eq!(extract(&mut narrow()).await.unwrap(), "ABC\n");
}

#[tokio::test]
async fn wide_ranges_are_rejected_as_an_ordinary_node_error() {
    for (low, high, codes) in [
        ("000001", "FFFFFE", vec![0, 0, 1]),
        ("00000001", "FFFFFFFE", vec![0, 0, 0, 1]),
    ] {
        let started = std::time::Instant::now();
        let error = extract(&mut range_pdf(low, high, codes))
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains(CMAP_LIMIT), "{low}: {error}");
        assert!(error.contains("page 1"), "{low}: {error}");
        assert!(
            started.elapsed() < std::time::Duration::from_secs(5),
            "{low}: rejection took {:?}",
            started.elapsed()
        );
    }
}

/// The finding's three PDFs through the CLI with `:timeout(5)`: the child's own
/// peak memory is compared with the narrow run, so the bound holds on any host.
#[cfg(unix)]
#[test]
fn wide_ranges_fail_fast_with_bounded_memory_in_the_cli() {
    const HEADROOM: u64 = 48 * 1024 * 1024;
    let temp = tempfile::tempdir().unwrap();
    let baseline = measured::run_measured(&fixture::save(&mut narrow(), temp.path()), PATIENCE);
    assert!(baseline.success, "{}", baseline.output);
    assert!(baseline.output.contains("TEXT=[ABC"), "{}", baseline.output);
    // An unbounded range grows until it is killed, so the wide runs get a
    // deadline scaled from this host's own narrow run instead of a long one.
    let limit = baseline.elapsed * 4 + std::time::Duration::from_secs(5);

    for (low, high, codes) in [
        ("000001", "FFFFFE", vec![0, 0, 1]),
        ("00000001", "FFFFFFFE", vec![0, 0, 0, 1]),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let path = fixture::save(&mut range_pdf(low, high, codes), temp.path());
        let run = measured::run_measured(&path, limit);
        assert!(!run.killed, "{low}: still running after {limit:?}");
        assert!(!run.success, "{low}: {}", run.output);
        assert!(run.output.contains(CMAP_LIMIT), "{low}: {}", run.output);
        assert!(
            run.peak_rss_bytes <= baseline.peak_rss_bytes + HEADROOM,
            "{low}: peak {} bytes against a {} byte baseline",
            run.peak_rss_bytes,
            baseline.peak_rss_bytes
        );
    }
}

#[tokio::test]
async fn a_font_shared_by_many_pages_is_decoded_and_charged_once() {
    // Twenty pages of one full two-byte font: 65,536 entries charged once fit
    // the default budget, charged per page they would not.
    let pages = (0..20).map(|_| show(1, vec![0, 0])).collect();
    let mut doc = document(&[range("0000", "FFFF")], pages);
    assert_eq!(extract(&mut doc).await.unwrap(), "A\n".repeat(20));
}

#[tokio::test]
async fn distinct_fonts_share_one_budget_for_the_call() {
    // Seventeen full two-byte fonts exceed 1,048,576 entries; sixteen do not
    // only because each CMap also charges its codespace line.
    let cmaps = vec![range("0000", "FFFF"); 17];
    let error = extract(&mut document(&cmaps, vec![show(1, vec![0, 0])]))
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains(CMAP_LIMIT), "{error}");
    assert!(error.contains("1048576"), "{error}");

    let cmaps = vec![range("0000", "FFFF"); 15];
    let text = extract(&mut document(&cmaps, vec![show(15, vec![0, 0])]))
        .await
        .unwrap();
    assert_eq!(text, "A\n");
}

#[tokio::test]
async fn a_long_destination_still_decodes_exactly_within_the_budget() {
    let mut doc = document(&[long_target(256)], vec![show(1, vec![1; 1000])]);
    let text = extract(&mut doc).await.unwrap();
    assert_eq!(text.len(), 256_001);
    assert!(text.trim_end().bytes().all(|byte| byte == b'A'));
}

#[tokio::test]
async fn a_long_glyph_string_is_rejected_before_it_is_decoded() {
    // 256 KiB of a code that maps to 256 units would decode to 64 MiB.
    let mut doc = document(&[long_target(256)], vec![show(1, vec![1; 256 * 1024])]);
    let error = extract(&mut doc).await.unwrap_err().to_string();
    assert!(error.contains(OUTPUT_LIMIT), "{error}");
    assert!(error.contains("page 1"), "{error}");
    assert!(error.contains("glyph string"), "{error}");
}

/// The same PDF through the CLI: decoding the operand first costs several
/// hundred megabytes before the output limit is compared.
#[cfg(unix)]
#[test]
fn a_long_glyph_string_fails_with_bounded_memory_in_the_cli() {
    const HEADROOM: u64 = 48 * 1024 * 1024;
    let limit = PATIENCE;
    let temp = tempfile::tempdir().unwrap();
    let baseline = measured::run_measured(&fixture::save(&mut narrow(), temp.path()), limit);
    assert!(baseline.success, "{}", baseline.output);

    let temp = tempfile::tempdir().unwrap();
    let mut doc = document(&[long_target(256)], vec![show(1, vec![1; 256 * 1024])]);
    let run = measured::run_measured(&fixture::save(&mut doc, temp.path()), limit);
    assert!(!run.killed && !run.success, "{}", run.output);
    assert!(run.output.contains(OUTPUT_LIMIT), "{}", run.output);
    assert!(
        run.peak_rss_bytes <= baseline.peak_rss_bytes + HEADROOM,
        "peak {} bytes against a {} byte baseline",
        run.peak_rss_bytes,
        baseline.peak_rss_bytes
    );
}
