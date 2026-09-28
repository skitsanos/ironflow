#[path = "pdf_pages/fixture.rs"]
mod fixture;

use std::path::Path;
use std::time::Duration;

use tokio::process::Command;

async fn checked(command: &mut Command) -> std::process::Output {
    let output = tokio::time::timeout(Duration::from_secs(30), command.kill_on_drop(true).output())
        .await
        .expect("native PDF command timed out")
        .expect("native PDF command unavailable");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

async fn render(source: &Path, page: u32, prefix: &Path) -> image::RgbImage {
    checked(
        Command::new("pdftoppm")
            .args([
                "-f",
                &page.to_string(),
                "-l",
                &page.to_string(),
                "-r",
                "72",
                "-cropbox",
                "-singlefile",
                "-png",
            ])
            .arg(source)
            .arg(prefix),
    )
    .await;
    image::open(prefix.with_extension("png")).unwrap().to_rgb8()
}

#[tokio::test]
#[ignore = "requires pdftoppm and pdftotext (Poppler) on PATH"]
async fn cli_split_and_merge_preserve_inherited_page_rendering() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("source.pdf");
    fixture::inherited_document().save(&source).unwrap();
    let pages = directory.path().join("pages");
    let merged = directory.path().join("merged.pdf");
    let flow = directory.path().join("page-fidelity.lua");
    let quoted = |path: &Path| serde_json::to_string(path.to_str().unwrap()).unwrap();
    std::fs::write(&flow, format!(
        "local flow = Flow.new('page_fidelity')\nflow:step('split', nodes.pdf_split({{path = {}, output_dir = {}, pages = '1'}}))\nflow:step('merge', nodes.pdf_merge({{files = {{{}, {}}}, output_path = {}}}))\nreturn flow\n",
        quoted(&source), quoted(&pages), quoted(&source), quoted(&source), quoted(&merged)
    )).unwrap();
    let output = checked(
        Command::new(env!("CARGO_BIN_EXE_ironflow"))
            .env_clear()
            .current_dir(directory.path())
            .arg("run")
            .arg(&flow)
            .arg("--store-dir")
            .arg(directory.path().join("store")),
    )
    .await;
    assert!(String::from_utf8_lossy(&output.stdout).contains("Status: success"));
    let selected = pages.join("source_1.pdf");
    let original_first = render(&source, 1, &directory.path().join("original-first")).await;
    let original_second = render(&source, 2, &directory.path().join("original-second")).await;
    assert_eq!(original_first.dimensions(), (140, 220));
    assert_eq!(original_second.dimensions(), (300, 200));
    assert!(
        original_first
            .pixels()
            .filter(|pixel| pixel.0 != [255, 255, 255])
            .count()
            > 1000
    );
    let split = render(&selected, 1, &directory.path().join("split-first")).await;
    assert_eq!(split.dimensions(), original_first.dimensions());
    assert!(split == original_first, "split changed rendered pixels");
    for page in 1..=4 {
        let rendered = render(
            &merged,
            page,
            &directory.path().join(format!("merged-{page}")),
        )
        .await;
        let expected = if page % 2 == 1 {
            &original_first
        } else {
            &original_second
        };
        assert_eq!(rendered.dimensions(), expected.dimensions());
        assert!(
            &rendered == expected,
            "merged page {page} changed rendered pixels"
        );
    }
    for pdf in [&selected, &merged] {
        let text = checked(
            Command::new("pdftotext")
                .args(["-f", "1", "-l", "1"])
                .arg(pdf)
                .arg("-"),
        )
        .await;
        assert!(String::from_utf8_lossy(&text.stdout).contains("SELECTED_PAGE_ONE"));
    }
    if std::env::var_os("IRONFLOW_KEEP_PDF_TEST_OUTPUT").is_some() {
        println!("PDF rendering evidence: {}", directory.keep().display());
    }
}

#[tokio::test]
#[ignore = "requires pdftoppm (Poppler) on PATH"]
async fn cli_grouped_split_preserves_reordered_page_pixels() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("source.pdf");
    fixture::inherited_document().save(&source).unwrap();
    let pages = directory.path().join("parts");
    let flow = directory.path().join("grouped.lua");
    std::fs::write(&flow, format!(
        "local flow = Flow.new('grouped_fidelity')\nflow:step('split', nodes.pdf_split({{path = {}, output_dir = {}, pages = '2,1', pages_per_file = 2}}))\nreturn flow\n",
        serde_json::json!(source), serde_json::json!(pages)
    )).unwrap();
    checked(
        Command::new(env!("CARGO_BIN_EXE_ironflow"))
            .env_clear()
            .current_dir(directory.path())
            .arg("run")
            .arg(&flow),
    )
    .await;
    let grouped = pages.join("source_part_001.pdf");
    for original_page in 1..=2 {
        let original = render(
            &source,
            original_page,
            &directory.path().join(format!("original-{original_page}")),
        )
        .await;
        let result = render(
            &grouped,
            3 - original_page,
            &directory.path().join(format!("grouped-{original_page}")),
        )
        .await;
        assert!(
            original
                .pixels()
                .filter(|pixel| pixel.0 != [255, 255, 255])
                .count()
                > 1000
        );
        assert_eq!(result.dimensions(), original.dimensions());
        assert!(
            result == original,
            "grouping changed page {original_page} pixels"
        );
    }
    if std::env::var_os("IRONFLOW_KEEP_PDF_TEST_OUTPUT").is_some() {
        println!(
            "Grouped PDF rendering evidence: {}",
            directory.keep().display()
        );
    }
}
