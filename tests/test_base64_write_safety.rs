use base64::Engine;
use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;
use serde_json::json;

#[tokio::test]
async fn binary_file_example_validates_and_runs_with_isolated_outputs() {
    let directory = tempfile::tempdir().unwrap();
    let scratch = directory.path().join("scratch");
    std::fs::create_dir(&scratch).unwrap();
    let example = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("examples/04-file-operations/binary_file_io.lua");
    for action in ["validate", "run"] {
        let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_ironflow"));
        command
            .env_clear()
            .current_dir(directory.path())
            .kill_on_drop(true)
            .env("TMPDIR", &scratch)
            .env("IRONFLOW_ARTIFACT_DIR", directory.path().join("artifacts"))
            .arg(action)
            .arg(&example);
        if action == "validate" {
            command.arg("--strict");
        }
        let result = tokio::time::timeout(std::time::Duration::from_secs(15), command.output())
            .await
            .unwrap()
            .unwrap();
        let stdout = String::from_utf8(result.stdout).unwrap();
        assert!(
            result.status.success(),
            "{stdout}\n{}",
            String::from_utf8_lossy(&result.stderr)
        );
        if action == "run" {
            assert!(stdout.contains("Status: success"), "{stdout}");
            let (_, context) = stdout.split_once("\nContext:\n").unwrap();
            let context: serde_json::Value = serde_json::from_str(context).unwrap();
            assert_eq!(context["roundtrip_ok"], true);
            assert_eq!(context["artifact_roundtrip_ok"], true);
        }
    }
    assert_eq!(std::fs::read_dir(scratch).unwrap().count(), 0);
}

#[tokio::test]
async fn expired_base64_write_preserves_the_destination() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("payload.bin");
    std::fs::write(&path, b"original").unwrap();
    let registry = NodeRegistry::with_builtins();
    let node = registry.get("base64_decode").unwrap();
    let config = json!({"output_file": path, "input": "cmVwbGFjZWQ="});
    let error = ironflow::util::execution::with_execution_deadline(
        Some(tokio::time::Instant::now()),
        node.execute(&config, &Context::new()),
    )
    .await
    .unwrap_err();
    assert!(format!("{error:#}").contains("deadline exceeded"));
    assert_eq!(std::fs::read(path).unwrap(), b"original");
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
}

#[cfg(unix)]
#[tokio::test]
async fn base64_refuses_fifo_without_a_reader() {
    use std::os::unix::ffi::OsStrExt;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("fifo");
    let name = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
    let registry = NodeRegistry::with_builtins();
    let node = registry.get("base64_decode").unwrap();
    let config = json!({"output_file": path, "input": "cmVwbGFjZWQ="});
    let error = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        node.execute(&config, &Context::new()),
    )
    .await
    .expect("FIFO write must not wait for a reader")
    .unwrap_err();
    assert!(format!("{error:#}").contains("regular file"), "{error:#}");
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
}

#[tokio::test]
async fn base64_replaces_regular_file_and_keeps_binary_output() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("payload.bin");
    std::fs::write(&path, b"original").unwrap();
    let bytes = [0xfb, 0xff, 0, 0x80];
    let output = NodeRegistry::with_builtins().get("base64_decode").unwrap()
        .execute(&json!({"output_file": path, "input": base64::engine::general_purpose::URL_SAFE.encode(bytes), "url_safe": true}), &Context::new()).await.unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    assert_eq!(output["base64_decoded_path"], path.to_str().unwrap());
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
}

#[tokio::test]
async fn malformed_base64_preserves_existing_file() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("payload.bin");
    std::fs::write(&path, b"original").unwrap();
    assert!(
        NodeRegistry::with_builtins()
            .get("base64_decode")
            .unwrap()
            .execute(
                &json!({"output_file": path, "input": "!!!!"}),
                &Context::new()
            )
            .await
            .is_err()
    );
    assert_eq!(std::fs::read(&path).unwrap(), b"original");
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
}

#[cfg(unix)]
#[tokio::test]
async fn base64_refuses_existing_and_dangling_destination_symlinks() {
    use std::os::unix::fs::symlink;
    for existing in [true, false] {
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("target");
        let destination = directory.path().join("link");
        if existing {
            std::fs::write(&target, b"sentinel").unwrap();
        }
        symlink(&target, &destination).unwrap();
        let error = NodeRegistry::with_builtins()
            .get("base64_decode")
            .unwrap()
            .execute(
                &json!({"output_file": destination, "input": "cmVwbGFjZWQ="}),
                &Context::new(),
            )
            .await
            .unwrap_err();
        assert!(format!("{error:#}").contains("symlink"), "{error:#}");
        assert!(
            std::fs::symlink_metadata(destination)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        if existing {
            assert_eq!(std::fs::read(target).unwrap(), b"sentinel");
        } else {
            assert!(!target.exists());
        }
        assert_eq!(
            std::fs::read_dir(directory.path()).unwrap().count(),
            if existing { 2 } else { 1 }
        );
    }
}
