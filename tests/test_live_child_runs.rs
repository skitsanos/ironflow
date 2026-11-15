//! IF-215: the number of live child runs is bounded for the whole process,
//! so fan-out at several nesting levels cannot multiply without limit.

use std::fs;
use std::path::Path;
use std::process::Command;

/// `root.lua` fans out to `width` copies of `mid.lua`, each of which fans
/// out to `width` copies of `leaf.lua`. Every leaf pauses, so all
/// `width + width * width` child runs are alive at the same time.
fn write_fan_out(dir: &Path, width: usize) {
    let fan_out = |name: &str, child: &str| {
        let flows: Vec<String> = (0..width)
            .map(|_| format!("{{ flow = \"{child}.lua\" }}"))
            .collect();
        format!(
            r#"
            local flow = Flow.new("{name}")
            flow:step("fan", nodes.parallel_subworkflows({{
                flows = {{ {} }},
                max_concurrent = {width},
            }}))
            return flow
            "#,
            flows.join(", ")
        )
    };
    fs::write(dir.join("root.lua"), fan_out("root", "mid")).unwrap();
    fs::write(dir.join("mid.lua"), fan_out("mid", "leaf")).unwrap();
    fs::write(
        dir.join("leaf.lua"),
        r#"
        local flow = Flow.new("leaf")
        flow:step("pause", nodes.delay({ seconds = 1.5 }))
        return flow
        "#,
    )
    .unwrap();
}

fn run(dir: &Path, limit: &str) -> (bool, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_ironflow"))
        .current_dir(dir)
        .env_clear()
        .env("TMPDIR", dir)
        .env("IRONFLOW_ARTIFACT_DIR", dir.join("artifacts"))
        .env("IRONFLOW_MAX_LIVE_CHILD_RUNS", limit)
        .args(["run", "root.lua"])
        .output()
        .unwrap();
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    (output.status.success(), text)
}

#[test]
fn fan_out_above_the_live_child_run_limit_fails_the_step() {
    let dir = tempfile::tempdir().unwrap();
    write_fan_out(dir.path(), 3);

    // 3 + 9 child runs are wanted; 6 may be alive at once.
    let (succeeded, text) = run(dir.path(), "6");

    assert!(!succeeded, "{text}");
    assert!(
        text.contains(
            "child workflow run limit reached: 6 child runs are live \
             (IRONFLOW_MAX_LIVE_CHILD_RUNS)"
        ),
        "{text}"
    );
}

#[test]
fn fan_out_within_the_live_child_run_limit_succeeds() {
    let dir = tempfile::tempdir().unwrap();
    write_fan_out(dir.path(), 3);

    let (succeeded, text) = run(dir.path(), "12");

    assert!(succeeded, "{text}");
}

#[test]
fn a_malformed_live_child_run_limit_is_rejected_at_startup() {
    let dir = tempfile::tempdir().unwrap();
    write_fan_out(dir.path(), 1);

    let (succeeded, text) = run(dir.path(), "many");

    assert!(!succeeded, "{text}");
    assert!(
        text.contains("IRONFLOW_MAX_LIVE_CHILD_RUNS must be a positive integer"),
        "{text}"
    );
}
