use std::path::Path;
use std::time::Duration;

use base64::Engine;
use tokio::process::Command;

fn source(path: &Path, width: u32, height: u32) {
    image::RgbImage::from_pixel(width, height, image::Rgb([32, 128, 224]))
        .save(path)
        .unwrap();
}

async fn run(
    directory: &Path,
    input: &str,
    selector: &str,
    dimensions: &str,
    limit: u64,
) -> String {
    let flow = directory.join("resize.lua");
    std::fs::write(
        &flow,
        format!(
            "local flow = Flow.new('resize_safety')\n{input}\nflow:step('resize', nodes.image_resize({{{selector}, output_path = 'output.png', {dimensions}}})):depends_on('read')\nreturn flow\n"
        ),
    )
    .unwrap();
    let output = tokio::time::timeout(
        Duration::from_secs(30),
        Command::new(env!("CARGO_BIN_EXE_ironflow"))
            .env_clear()
            .env(
                "IRONFLOW_MAX_IMAGE_DECODE_ALLOCATION_BYTES",
                limit.to_string(),
            )
            .env("IRONFLOW_ARTIFACT_DIR", directory.join("artifacts"))
            .current_dir(directory)
            .arg("run")
            .arg(flow)
            .arg("--store-dir")
            .arg(directory.join("store"))
            .kill_on_drop(true)
            .output(),
    )
    .await
    .expect("resize CLI timed out")
    .expect("resize CLI failed to start");
    format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

const PATH_INPUT: &str =
    "flow:step('read', nodes.code({ source = [[ return { input = 'source.ppm' } ]] }))";

#[tokio::test]
async fn crossed_dimensions_reject_before_output_for_paths_base64_and_artifacts() {
    for kind in ["path", "base64", "artifact"] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path();
        source(&path.join("source.ppm"), 100, 1);
        let input = match kind {
            "path" => PATH_INPUT.to_owned(),
            "base64" => format!(
                "flow:step('read', nodes.code({{ source = [[ return {{ input = {{ base64 = '{}' }} }} ]] }}))",
                base64::engine::general_purpose::STANDARD
                    .encode(std::fs::read(path.join("source.ppm")).unwrap())
            ),
            _ => "flow:step('read', nodes.read_file({ path = 'source.ppm', encoding = 'artifact', output_key = 'input' }))".to_owned(),
        };
        let selector = match kind {
            "path" => "path = 'source.ppm'",
            "base64" => "source_key = 'input'",
            _ => "source_key = 'input_artifact'",
        };
        std::fs::write(path.join("output.png"), b"existing output").unwrap();
        let output = run(path, &input, selector, "width = 1, height = 100", 1024).await;
        assert!(
            output.contains("IRONFLOW_MAX_IMAGE_DECODE_ALLOCATION_BYTES"),
            "{kind}: {output}"
        );
        assert_eq!(
            std::fs::read(path.join("output.png")).unwrap(),
            b"existing output"
        );
    }
}

#[tokio::test]
async fn truncated_pixels_are_rejected_by_resize_admission_before_decode() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path();
    std::fs::write(path.join("source.ppm"), b"P6\n100 1\n255\n").unwrap();
    let output = run(
        path,
        PATH_INPUT,
        "path = 'source.ppm'",
        "width = 1, height = 100",
        1024,
    )
    .await;
    assert!(output.contains("resampling intermediate"), "{output}");
    assert!(!path.join("output.png").exists());
    let output = run(
        path,
        PATH_INPUT,
        "path = 'source.ppm'",
        "width = 1, height = 100",
        200_000,
    )
    .await;
    assert!(output.contains("invalid image data"), "{output}");
    assert!(!path.join("output.png").exists());
}

#[tokio::test]
async fn ordinary_resizes_and_same_size_copy_fit_the_small_budget() {
    for (width, height, dimensions, expected) in [
        (4, 2, "width = 2", (2, 1)),
        (4, 2, "height = 1", (2, 1)),
        (4, 2, "width = 3, height = 3", (3, 3)),
        (100, 1, "width = 100", (100, 1)),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path();
        source(&path.join("source.ppm"), width, height);
        let output = run(path, PATH_INPUT, "source_key = 'input'", dimensions, 1024).await;
        assert!(output.contains("Status: success"), "{output}");
        let resized = image::open(path.join("output.png")).unwrap().to_rgb8();
        assert_eq!(resized.dimensions(), expected);
        assert!(resized.pixels().all(|pixel| pixel.0 == [32, 128, 224]));
    }
}

#[tokio::test]
async fn crossed_resize_succeeds_with_sufficient_budget_and_verified_artifact() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path();
    source(&path.join("source.ppm"), 100, 1);
    let input = "flow:step('read', nodes.read_file({ path = 'source.ppm', encoding = 'artifact', output_key = 'input' }))";
    let output = run(
        path,
        input,
        "source_key = 'input_artifact'",
        "width = 1, height = 100",
        200_000,
    )
    .await;
    assert!(output.contains("Status: success"), "{output}");
    let resized = image::open(path.join("output.png")).unwrap().to_rgb8();
    assert_eq!(resized.dimensions(), (1, 100));
    assert!(resized.pixels().all(|pixel| pixel.0 == [32, 128, 224]));
}

#[tokio::test]
async fn expired_resize_deadline_does_not_write_an_output() {
    let directory = tempfile::tempdir().unwrap();
    let source_path = directory.path().join("source.ppm");
    source(&source_path, 4, 2);
    let output_path = directory.path().join("output.png");
    let registry = ironflow::nodes::NodeRegistry::with_builtins();
    let error = ironflow::util::execution::with_execution_deadline(
        Some(tokio::time::Instant::now() - Duration::from_secs(1)),
        registry.get("image_resize").unwrap().execute(
            &serde_json::json!({"path": source_path, "output_path": output_path, "width": 2}),
            &ironflow::engine::types::Context::new(),
        ),
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("deadline"));
    assert!(!output_path.exists());
}
