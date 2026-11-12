//! IF-185: a file published through the staged writer has the mode a plain
//! write would have left. A new file gets the platform default for its
//! directory (`0666` masked by the umask), a replaced or appended file keeps
//! its permission bits, and `write_file` writes to one destination are
//! serialized inside the process. The nodes run in-process through the
//! registry on scratch data.
#![cfg(unix)]

use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;
use serde_json::{Value, json};

fn scratch() -> (tempfile::TempDir, PathBuf) {
    let directory = tempfile::tempdir().unwrap();
    let work = directory.path().join("work");
    std::fs::create_dir(&work).unwrap();
    (directory, work)
}

fn mode(path: &Path) -> u32 {
    std::fs::metadata(path).unwrap().permissions().mode() & 0o7777
}

fn set_mode(path: &Path, mode: u32) {
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
}

/// The mode a plain `File::create` leaves in `directory` under this
/// process's umask, measured instead of assumed.
fn default_mode(directory: &Path) -> u32 {
    let probe = directory.join("plain-create-probe");
    std::fs::File::create(&probe).unwrap();
    let measured = mode(&probe);
    std::fs::remove_file(&probe).unwrap();
    measured
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

async fn execute(node: &str, config: Value) {
    NodeRegistry::with_builtins()
        .get(node)
        .unwrap()
        .execute(&config, &Context::new())
        .await
        .unwrap_or_else(|error| panic!("{node} failed: {error:#}"));
}

fn write_zip(path: &Path, entries: &[(&str, u32)]) {
    let mut archive = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    for (name, permissions) in entries {
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored)
            .unix_permissions(*permissions);
        archive.start_file(*name, options).unwrap();
        archive.write_all(b"archive payload").unwrap();
    }
    archive.finish().unwrap();
}

#[tokio::test]
async fn write_file_creates_a_new_file_with_the_default_mode() {
    let (_directory, work) = scratch();
    let expected = default_mode(&work);
    let destination = work.join("nested/new.txt");

    execute("write_file", json!({"path": destination, "content": "x"})).await;

    assert_eq!(
        format!("{:o}", mode(&destination)),
        format!("{expected:o}"),
        "a new file must have the mode of a plain create"
    );
    assert_eq!(staged_leftovers(&work.join("nested")), 0);
}

#[tokio::test]
async fn write_file_overwrite_keeps_the_existing_mode() {
    let (_directory, work) = scratch();
    for existing in [0o640, 0o644, 0o755, 0o400] {
        let destination = work.join(format!("replaced-{existing:o}.txt"));
        std::fs::write(&destination, b"before").unwrap();
        set_mode(&destination, existing);

        execute(
            "write_file",
            json!({"path": destination, "content": "after"}),
        )
        .await;

        assert_eq!(std::fs::read(&destination).unwrap(), b"after");
        assert_eq!(
            format!("{:o}", mode(&destination)),
            format!("{existing:o}"),
            "an overwritten file must keep its permission bits"
        );
    }
    assert_eq!(staged_leftovers(&work), 0);
}

#[tokio::test]
async fn write_file_append_keeps_an_executable_mode() {
    let (_directory, work) = scratch();
    let destination = work.join("script.sh");
    std::fs::write(&destination, b"#!/bin/sh\n").unwrap();
    set_mode(&destination, 0o755);

    execute(
        "write_file",
        json!({"path": destination, "content": "exit 0\n", "append": true}),
    )
    .await;

    assert_eq!(std::fs::read(&destination).unwrap(), b"#!/bin/sh\nexit 0\n");
    assert_eq!(
        format!("{:o}", mode(&destination)),
        "755",
        "an appended script must stay executable"
    );
    assert_eq!(staged_leftovers(&work), 0);
}

#[tokio::test]
async fn append_to_a_missing_file_creates_it_with_the_default_mode() {
    let (_directory, work) = scratch();
    let expected = default_mode(&work);
    let destination = work.join("log.txt");

    execute(
        "write_file",
        json!({"path": destination, "content": "first\n", "append": true}),
    )
    .await;

    assert_eq!(std::fs::read(&destination).unwrap(), b"first\n");
    assert_eq!(format!("{:o}", mode(&destination)), format!("{expected:o}"));
}

#[tokio::test]
async fn a_replaced_file_does_not_keep_setuid_setgid_or_sticky_bits() {
    let (_directory, work) = scratch();
    let destination = work.join("tool");
    std::fs::write(&destination, b"before").unwrap();
    set_mode(&destination, 0o4755);
    if mode(&destination) != 0o4755 {
        eprintln!("skipping: this filesystem does not keep a setuid bit");
        return;
    }

    execute(
        "write_file",
        json!({"path": destination, "content": "after"}),
    )
    .await;

    assert_eq!(
        format!("{:o}", mode(&destination)),
        "755",
        "only the nine permission bits are carried to the replacement"
    );
}

#[tokio::test]
async fn base64_decode_output_file_follows_the_same_mode_contract() {
    let (_directory, work) = scratch();
    let expected = default_mode(&work);
    let created = work.join("decoded.bin");
    let replaced = work.join("replaced.bin");
    std::fs::write(&replaced, b"before").unwrap();
    set_mode(&replaced, 0o640);

    for destination in [&created, &replaced] {
        execute(
            "base64_decode",
            json!({"input": "aGVsbG8=", "output_file": destination}),
        )
        .await;
        assert_eq!(std::fs::read(destination).unwrap(), b"hello");
    }

    assert_eq!(format!("{:o}", mode(&created)), format!("{expected:o}"));
    assert_eq!(format!("{:o}", mode(&replaced)), "640");
}

#[tokio::test]
async fn copy_file_keeps_the_source_mode_over_an_existing_destination() {
    let (_directory, work) = scratch();
    let source = work.join("source.sh");
    let destination = work.join("destination.sh");
    std::fs::write(&source, b"#!/bin/sh\nexit 0\n").unwrap();
    set_mode(&source, 0o751);
    std::fs::write(&destination, b"old").unwrap();
    set_mode(&destination, 0o600);

    execute(
        "copy_file",
        json!({"source": source, "destination": destination}),
    )
    .await;

    assert_eq!(
        format!("{:o}", mode(&destination)),
        "751",
        "copy_file publishes the source mode (IF-140), not the replaced file's"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn parallel_appends_to_one_file_keep_every_line() {
    const WRITERS: usize = 32;
    let (_directory, work) = scratch();
    let destination = work.join("events.log");
    // A base large enough that every unserialized append overlaps another.
    let base = "base line\n".repeat(20_000);
    std::fs::write(&destination, &base).unwrap();
    set_mode(&destination, 0o640);

    let mut writers = Vec::with_capacity(WRITERS);
    for writer in 0..WRITERS {
        let destination = destination.clone();
        writers.push(tokio::spawn(async move {
            execute(
                "write_file",
                json!({
                    "path": destination,
                    "content": format!("writer {writer}\n"),
                    "append": true
                }),
            )
            .await;
        }));
    }
    for writer in writers {
        writer.await.unwrap();
    }

    let contents = std::fs::read_to_string(&destination).unwrap();
    let appended = contents.strip_prefix(base.as_str()).unwrap_or_else(|| {
        panic!("the existing content was not kept as the prefix of the appended file")
    });
    let mut lines: Vec<&str> = appended.lines().collect();
    assert_eq!(
        lines.len(),
        WRITERS,
        "every successful append must be present; found {} of {WRITERS} lines",
        lines.len()
    );
    lines.sort_unstable();
    lines.dedup();
    assert_eq!(lines.len(), WRITERS, "an append was duplicated");
    assert_eq!(format!("{:o}", mode(&destination)), "640");
    assert_eq!(staged_leftovers(&work), 0);
}

#[tokio::test]
async fn zip_create_follows_the_mode_contract() {
    let (directory, work) = scratch();
    let expected = default_mode(&work);
    let source = directory.path().join("source");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(source.join("a.txt"), b"a").unwrap();
    let created = work.join("created.zip");
    let replaced = work.join("replaced.zip");
    std::fs::write(&replaced, b"old archive").unwrap();
    set_mode(&replaced, 0o640);

    for zip_path in [&created, &replaced] {
        execute(
            "zip_create",
            json!({"source": source, "zip_path": zip_path}),
        )
        .await;
    }

    assert_eq!(format!("{:o}", mode(&created)), format!("{expected:o}"));
    assert_eq!(format!("{:o}", mode(&replaced)), "640");
    assert_eq!(staged_leftovers(&work), 0);
}

#[tokio::test]
async fn zip_extract_ignores_archive_modes_and_keeps_replaced_ones() {
    let (directory, work) = scratch();
    let expected = default_mode(&work);
    let archive = directory.path().join("archive.zip");
    write_zip(
        &archive,
        &[
            ("plain.txt", 0o644),
            ("nested/script.sh", 0o755),
            ("setuid-world-writable", 0o6777),
            ("private.txt", 0o600),
            ("replaced.txt", 0o777),
        ],
    );
    std::fs::write(work.join("replaced.txt"), b"before").unwrap();
    set_mode(&work.join("replaced.txt"), 0o640);

    execute("zip_extract", json!({"path": archive, "destination": work})).await;

    for created in [
        "plain.txt",
        "nested/script.sh",
        "setuid-world-writable",
        "private.txt",
    ] {
        assert_eq!(
            format!("{:o}", mode(&work.join(created))),
            format!("{expected:o}"),
            "{created}: a new entry gets the default mode, never the archive's"
        );
    }
    assert_eq!(
        std::fs::read(work.join("replaced.txt")).unwrap(),
        b"archive payload"
    );
    assert_eq!(format!("{:o}", mode(&work.join("replaced.txt"))), "640");
    assert_eq!(staged_leftovers(&work), 0);
}

/// Run one `write_file` flow through the CLI in a child process whose umask
/// is `mask`, so no umask of this test process changes.
async fn write_under_umask(directory: &Path, mask: libc::mode_t) -> String {
    let flow = directory.join("flow.lua");
    std::fs::write(
        &flow,
        "local flow = Flow.new('umask_modes')\n\
         flow:step('write', nodes.write_file({ path = 'made/deeper/new.txt', content = 'x' }))\n\
         return flow\n",
    )
    .unwrap();
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_ironflow"));
    command
        .env_clear()
        .env("IRONFLOW_ARTIFACT_DIR", directory.join("artifacts"))
        .current_dir(directory)
        .arg("run")
        .arg(&flow)
        .arg("--store-dir")
        .arg(directory.join("store"))
        .kill_on_drop(true);
    // SAFETY: `umask` is async-signal-safe and only changes the child.
    unsafe {
        command.pre_exec(move || {
            libc::umask(mask);
            Ok(())
        });
    }
    let output = tokio::time::timeout(std::time::Duration::from_secs(30), command.output())
        .await
        .expect("ironflow CLI timed out")
        .expect("ironflow CLI failed to start");
    format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

#[tokio::test]
async fn a_new_file_is_0666_masked_by_the_process_umask() {
    for (mask, file_mode, directory_mode) in [
        (0o022, 0o644, 0o755),
        (0o027, 0o640, 0o750),
        (0o002, 0o664, 0o755),
        (0o077, 0o600, 0o700),
    ] {
        let directory = tempfile::tempdir().unwrap();

        let output = write_under_umask(directory.path(), mask).await;

        assert!(output.contains("Status: success"), "{output}");
        let made = directory.path().join("made");
        assert_eq!(
            format!("{:o}", mode(&made.join("deeper/new.txt"))),
            format!("{file_mode:o}"),
            "umask {mask:o}: new file"
        );
        for created in [made.clone(), made.join("deeper")] {
            assert_eq!(
                format!("{:o}", mode(&created)),
                format!("{directory_mode:o}"),
                "umask {mask:o}: created parent directories stay 0755 masked by the umask"
            );
        }
    }
}
