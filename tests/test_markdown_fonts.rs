//! Font availability contract for `markdown_to_pdf` (IF-164): CJK glyphs come
//! from the opt-in `cjk-fonts` feature or from a runtime `IRONFLOW_FONT_DIR`,
//! and their absence is a clear error rather than a missing-glyph failure.

use std::path::Path;
use std::process::{Command, Output};

const CJK_FLOW: &str = r##"local flow = Flow.new("cjk_fonts")
flow:step("pdf", nodes.markdown_to_pdf({
    input = "# 中文报告\n\n日本語の文書です。 한국어 문서입니다.",
    output_path = "cjk.pdf",
    output_key = "pdf",
}))
return flow
"##;

fn run(workdir: &Path, font_dir: Option<&Path>) -> Output {
    std::fs::write(workdir.join("flow.lua"), CJK_FLOW).unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_ironflow"));
    command
        .current_dir(workdir)
        .env_clear()
        .args(["run", "flow.lua"]);
    if let Some(dir) = font_dir {
        command.env("IRONFLOW_FONT_DIR", dir);
    }
    command.output().unwrap()
}

fn combined(output: &Output) -> String {
    format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

#[cfg(not(feature = "cjk-fonts"))]
#[test]
fn cjk_text_without_a_cjk_font_points_to_the_feature_or_font_directory() {
    let temp = tempfile::tempdir().unwrap();
    let output = run(temp.path(), None);
    assert_eq!(output.status.code(), Some(1));
    let text = combined(&output);
    for expected in ["no CJK font", "cjk-fonts", "IRONFLOW_FONT_DIR"] {
        assert!(text.contains(expected), "missing {expected:?} in: {text}");
    }
    assert!(!temp.path().join("cjk.pdf").exists());
}

#[cfg(feature = "cjk-fonts")]
#[test]
fn bundled_cjk_font_renders_without_a_font_directory() {
    let temp = tempfile::tempdir().unwrap();
    let output = run(temp.path(), None);
    assert!(output.status.success(), "{}", combined(&output));
    assert!(
        std::fs::metadata(temp.path().join("cjk.pdf"))
            .unwrap()
            .len()
            > 0
    );
}

#[test]
fn a_runtime_font_directory_supplies_cjk_glyphs() {
    let temp = tempfile::tempdir().unwrap();
    let fonts = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/fonts");
    let output = run(temp.path(), Some(&fonts));
    assert!(output.status.success(), "{}", combined(&output));
    assert!(
        std::fs::metadata(temp.path().join("cjk.pdf"))
            .unwrap()
            .len()
            > 0
    );
}

#[test]
fn an_unreadable_font_directory_is_reported_by_name() {
    let temp = tempfile::tempdir().unwrap();
    let missing = temp.path().join("no-such-fonts");
    let output = run(temp.path(), Some(&missing));
    assert_eq!(output.status.code(), Some(1));
    let text = combined(&output);
    assert!(
        text.contains("IRONFLOW_FONT_DIR") && text.contains("no-such-fonts"),
        "unexpected output: {text}"
    );
    assert!(!temp.path().join("cjk.pdf").exists());
}
