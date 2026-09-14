use std::path::Path;
use std::time::Duration;

use serde_json::Value;

#[path = "support/pptx.rs"]
mod fixture;
use fixture::{presentation, relationships, slide, write};

async fn run(
    directory: &Path,
    flow: &Path,
    extra: &[(&str, &str)],
    success: bool,
) -> std::process::Output {
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_ironflow"));
    command
        .env_clear()
        .current_dir(directory)
        .kill_on_drop(true)
        .env("TMPDIR", directory)
        .env("IRONFLOW_ARTIFACT_DIR", directory.join("artifacts"))
        .envs(extra.iter().copied())
        .arg("run")
        .arg(flow);
    let output = tokio::time::timeout(Duration::from_secs(20), command.output())
        .await
        .expect("PPTX CLI did not settle")
        .unwrap();
    assert_eq!(
        output.status.success(),
        success,
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn make_flow(directory: &Path) -> std::path::PathBuf {
    let flow = directory.join("read.lua");
    std::fs::write(&flow, "local flow = Flow.new('pptx-graph')\nflow:step('read', nodes.extract_pptx({path = 'deck.pptx', format = 'json', media_mode = 'artifact', comments_key = 'comments', metadata_key = 'metadata'}))\nreturn flow").unwrap();
    flow
}

#[tokio::test]
async fn cli_resolves_renamed_images_notes_comments_and_authors() {
    let directory = tempfile::tempdir().unwrap();
    let dir = directory.path();
    let path = dir.join("deck.pptx");
    write(
        &path,
        &[
            (
                "ppt/presentation.xml",
                &presentation(&[("900", "last"), ("256", "first")]),
            ),
            (
                "ppt/_rels/presentation.xml.rels",
                &relationships(&[
                    ("last", "slide", "../custom/pages/slide99.xml"),
                    ("first", "slide", "slides/slide20.xml"),
                    ("a", "commentAuthors", "/custom/review/authors.xml"),
                ]),
            ),
            (
                "custom/pages/slide99.xml",
                "<sld><pic><blip embed=\"img\"/></pic><pic><blip embed=\"img\"/></pic></sld>",
            ),
            (
                "custom/pages/_rels/slide99.xml.rels",
                &relationships(&[
                    ("img", "image", "../media/image.png"),
                    ("n", "notesSlide", "/custom/notes/note.xml"),
                    ("c", "comments", "../review/comment.xml"),
                ]),
            ),
            ("custom/media/image.png", "test-image-payload"),
            (
                "custom/notes/note.xml",
                "<notes><p><t>Linked note</t></p></notes>",
            ),
            (
                "custom/review/comment.xml",
                "<cmLst><cm authorId=\"7\"><text>Linked comment</text></cm></cmLst>",
            ),
            (
                "custom/review/authors.xml",
                "<cmAuthorLst><cmAuthor id=\"7\" name=\"Review &amp; Team\" initials=\"RT\"/></cmAuthorLst>",
            ),
            ("ppt/slides/slide20.xml", &slide("Second page")),
            ("ppt/slides/slide1.xml", &slide("ORPHAN")),
        ],
    );
    let output = run(dir, &make_flow(dir), &[], true).await;
    let stdout = String::from_utf8(output.stdout).unwrap();
    let context: Value =
        serde_json::from_str(stdout.split_once("\nContext:\n").unwrap().1.trim()).unwrap();
    let slides = context["content"]["slides"].as_array().unwrap();
    assert_eq!(slides.len(), 2);
    assert_eq!(slides[0]["slide_index"], 1);
    assert_eq!(slides[1]["slide_index"], 2);
    assert_eq!(context["metadata"]["slide_count"], 2);
    assert_eq!(slides[0]["speaker_notes"], "Linked note");
    assert_eq!(slides[0]["comments"], context["comments"]);
    assert_eq!(context["comments"][0]["author"], "Review & Team");
    assert_eq!(context["comments"][0]["slide_index"], 1);
    let first = &slides[0]["elements"][0];
    assert_eq!(first["embedded_path"], "custom/media/image.png");
    assert_eq!(first["artifact"], slides[0]["elements"][1]["artifact"]);
    assert_eq!(first["artifact"]["mime_type"], "image/png");
    let store = dir.join("artifacts/sha256");
    assert_eq!(std::fs::read_dir(&store).unwrap().count(), 1);
    assert_eq!(
        std::fs::read(store.join(first["artifact"]["sha256"].as_str().unwrap())).unwrap(),
        b"test-image-payload"
    );
    assert!(!serde_json::to_string(&context).unwrap().contains("ORPHAN"));
}

#[tokio::test]
async fn presentation_admission_limits_fail_before_media_publication() {
    for (name, value, expected) in [
        (
            "IRONFLOW_MAX_EXTRACT_ITEMS",
            "1",
            "IRONFLOW_MAX_EXTRACT_ITEMS",
        ),
        (
            "IRONFLOW_MAX_EXTRACT_OUTPUT_BYTES",
            "100",
            "extraction limit (100 bytes)",
        ),
        ("IRONFLOW_MAX_ZIP_ENTRIES", "2", "IRONFLOW_MAX_ZIP_ENTRIES"),
        (
            "IRONFLOW_MAX_ZIP_UNCOMPRESSED_BYTES",
            "100",
            "IRONFLOW_MAX_ZIP_UNCOMPRESSED_BYTES",
        ),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let dir = directory.path();
        write(
            &dir.join("deck.pptx"),
            &[
                ("ppt/presentation.xml", &presentation(&[("256", "s")])),
                (
                    "ppt/_rels/presentation.xml.rels",
                    &relationships(&[("s", "slide", "slides/slide1.xml")]),
                ),
                ("ppt/slides/slide1.xml", &slide("Bounded")),
            ],
        );
        let output = run(dir, &make_flow(dir), &[(name, value)], false).await;
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(text.contains(expected), "{text}");
        let store = dir.join("artifacts/sha256");
        assert!(!store.exists() || std::fs::read_dir(store).unwrap().count() == 0);
    }
}

#[tokio::test]
async fn later_missing_slide_is_detected_before_first_slide_publishes_media() {
    let directory = tempfile::tempdir().unwrap();
    let dir = directory.path();
    write(
        &dir.join("deck.pptx"),
        &[
            (
                "ppt/presentation.xml",
                &presentation(&[("256", "a"), ("257", "b")]),
            ),
            (
                "ppt/_rels/presentation.xml.rels",
                &relationships(&[
                    ("a", "slide", "slides/slide1.xml"),
                    ("b", "slide", "slides/missing.xml"),
                ]),
            ),
            (
                "ppt/slides/slide1.xml",
                "<sld><pic><blip embed=\"img\"/></pic></sld>",
            ),
            (
                "ppt/slides/_rels/slide1.xml.rels",
                &relationships(&[("img", "image", "../media/image.png")]),
            ),
            ("ppt/media/image.png", "not-published"),
        ],
    );
    let output = run(dir, &make_flow(dir), &[], false).await;
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("required archive part is missing")
            || String::from_utf8_lossy(&output.stderr).contains("required archive part is missing")
    );
    assert_eq!(
        std::fs::read_dir(dir.join("artifacts/sha256"))
            .unwrap()
            .count(),
        0
    );
}

#[tokio::test]
async fn committed_benchmark_deck_keeps_all_slides_and_deduplicates_media() {
    let directory = tempfile::tempdir().unwrap();
    let dir = directory.path();
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("benchmarks/extraction/fixtures/repeated-media.pptx"),
        dir.join("deck.pptx"),
    )
    .unwrap();
    let output = run(dir, &make_flow(dir), &[], true).await;
    let stdout = String::from_utf8(output.stdout).unwrap();
    let context: Value =
        serde_json::from_str(stdout.split_once("\nContext:\n").unwrap().1.trim()).unwrap();
    assert_eq!(context["metadata"]["slide_count"], 48);
    let slides = context["content"]["slides"].as_array().unwrap();
    assert_eq!(slides.len(), 48);
    let artifact = &slides[0]["elements"][0]["artifact"];
    for (index, slide) in slides.iter().enumerate() {
        assert_eq!(slide["slide_index"], index + 1);
        let images = slide["elements"].as_array().unwrap();
        assert_eq!(images.len(), 8);
        for image in images {
            assert_eq!(&image["artifact"], artifact);
        }
    }
    assert_eq!(
        std::fs::read_dir(dir.join("artifacts/sha256"))
            .unwrap()
            .count(),
        1
    );
}
