//! IF-140: `copy_file` and `move_file` share the `write_file` destination
//! policy. The flows run through the CLI binary so the whole node path,
//! including the tracked blocking stage, is exercised on scratch data.

use std::path::{Path, PathBuf};
use std::time::Duration;

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;
use serde_json::json;
use tokio::process::Command;

fn scratch() -> (tempfile::TempDir, PathBuf) {
    let directory = tempfile::tempdir().unwrap();
    let work = directory.path().join("work");
    std::fs::create_dir(&work).unwrap();
    (directory, work)
}

fn staged_leftovers(work: &Path) -> usize {
    std::fs::read_dir(work)
        .unwrap()
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with(".ironflow-")
        })
        .count()
}

async fn run_flow(directory: &Path, steps: &str, environment: &[(&str, &str)]) -> String {
    let flow = directory.join("flow.lua");
    std::fs::write(
        &flow,
        format!("local flow = Flow.new('copy_move_safety')\n{steps}\nreturn flow\n"),
    )
    .unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_ironflow"));
    command
        .env_clear()
        .env("IRONFLOW_ARTIFACT_DIR", directory.join("artifacts"))
        .envs(environment.iter().copied())
        .current_dir(directory)
        .arg("run")
        .arg(&flow)
        .arg("--store-dir")
        .arg(directory.join("store"))
        .kill_on_drop(true);
    let output = tokio::time::timeout(Duration::from_secs(30), command.output())
        .await
        .expect("ironflow CLI timed out")
        .expect("ironflow CLI failed to start");
    format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn step(node: &str) -> String {
    format!(
        "flow:step('op', nodes.{node}({{ source = 'work/source.txt', destination = 'work/destination' }}))"
    )
}

#[cfg(unix)]
#[tokio::test]
async fn copy_and_move_refuse_a_destination_symlink_and_leave_its_target_untouched() {
    use std::os::unix::fs::symlink;

    for node in ["copy_file", "move_file"] {
        let (directory, work) = scratch();
        std::fs::write(work.join("source.txt"), b"payload").unwrap();
        std::fs::write(work.join("target.txt"), b"sentinel").unwrap();
        symlink("target.txt", work.join("destination")).unwrap();

        let output = run_flow(directory.path(), &step(node), &[]).await;

        assert!(!output.contains("Status: success"), "{node}: {output}");
        assert!(output.contains(node), "{node}: {output}");
        assert!(output.contains("symlink"), "{node}: {output}");
        assert_eq!(std::fs::read(work.join("target.txt")).unwrap(), b"sentinel");
        assert_eq!(std::fs::read(work.join("source.txt")).unwrap(), b"payload");
        assert!(
            std::fs::symlink_metadata(work.join("destination"))
                .unwrap()
                .file_type()
                .is_symlink(),
            "{node}: the link itself must survive"
        );
        assert_eq!(staged_leftovers(&work), 0, "{node}");
    }
}

#[cfg(unix)]
#[tokio::test]
async fn copy_and_move_refuse_a_fifo_destination_without_blocking() {
    use std::os::unix::ffi::OsStrExt;

    for node in ["copy_file", "move_file"] {
        let (directory, work) = scratch();
        std::fs::write(work.join("source.txt"), b"payload").unwrap();
        let fifo = work.join("destination");
        let name = std::ffi::CString::new(fifo.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);

        let output = run_flow(directory.path(), &step(node), &[]).await;

        assert!(!output.contains("Status: success"), "{node}: {output}");
        assert!(output.contains("regular file"), "{node}: {output}");
        assert_eq!(std::fs::read(work.join("source.txt")).unwrap(), b"payload");
        assert_eq!(staged_leftovers(&work), 0, "{node}");
    }
}

#[tokio::test]
async fn plain_copy_and_move_succeed_and_replace_a_regular_destination() {
    let (directory, work) = scratch();
    std::fs::write(work.join("source.txt"), b"payload").unwrap();
    std::fs::write(work.join("copied.txt"), b"stale").unwrap();

    let steps = "\
flow:step('copy', nodes.copy_file({ source = 'work/source.txt', destination = 'work/copied.txt' }))
flow:step('move', nodes.move_file({ source = 'work/copied.txt', destination = 'work/nested/moved.txt' })):depends_on('copy')
flow:step('verify', nodes.read_file({ path = 'work/nested/moved.txt', output_key = 'moved' })):depends_on('move')";
    let output = run_flow(directory.path(), steps, &[]).await;

    assert!(output.contains("Status: success"), "{output}");
    assert!(
        output.contains("\"moved_content\": \"payload\""),
        "{output}"
    );
    assert_eq!(std::fs::read(work.join("source.txt")).unwrap(), b"payload");
    assert!(!work.join("copied.txt").exists());
    assert_eq!(
        std::fs::read(work.join("nested/moved.txt")).unwrap(),
        b"payload"
    );
    assert_eq!(staged_leftovers(&work), 0);
}

#[tokio::test]
async fn copy_is_bounded_by_the_file_byte_limit_before_staging() {
    let (directory, work) = scratch();
    std::fs::write(work.join("source.txt"), b"payload").unwrap();
    std::fs::write(work.join("destination"), b"existing").unwrap();

    let output = run_flow(
        directory.path(),
        &step("copy_file"),
        &[("IRONFLOW_MAX_FILE_BYTES", "4")],
    )
    .await;

    assert!(!output.contains("Status: success"), "{output}");
    assert!(output.contains("IRONFLOW_MAX_FILE_BYTES"), "{output}");
    assert_eq!(
        std::fs::read(work.join("destination")).unwrap(),
        b"existing"
    );
    assert_eq!(staged_leftovers(&work), 0);
}

#[cfg(unix)]
#[tokio::test]
async fn copy_refuses_a_source_symlink_like_read_file() {
    use std::os::unix::fs::symlink;

    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("target.txt"), b"secret").unwrap();
    let link = directory.path().join("link.txt");
    symlink("target.txt", &link).unwrap();
    let destination = directory.path().join("copied.txt");

    let error = NodeRegistry::with_builtins()
        .get("copy_file")
        .unwrap()
        .execute(
            &json!({"source": link, "destination": destination}),
            &Context::new(),
        )
        .await
        .unwrap_err();

    assert!(format!("{error:#}").contains("failed to open"), "{error:#}");
    assert!(!destination.exists());
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 2);
}
