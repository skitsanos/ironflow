//! IF-179: `transcribe` publishes `output_file` through the rooted,
//! no-follow, atomic writer inside the tracked blocking step, after the
//! provider call.

use std::path::{Path, PathBuf};
use std::time::Duration;

use axum::Router;
use axum::routing::post;
use ironflow::engine::types::{Context, NodeOutput};
use ironflow::nodes::NodeRegistry;
use tokio::time::Instant;

const VTT: &str = "WEBVTT\n\n00:00:00.000 --> 00:00:02.000\nHello there.\n";
const ORIGINAL: &[u8] = b"previous transcript";

/// A provider that answers with `VTT`, but not before `not_before`.
async fn start_provider(not_before: Option<Instant>) -> (String, tokio::task::JoinHandle<()>) {
    let app = Router::new().route(
        "/audio/transcriptions",
        post(move |_body: axum::body::Bytes| async move {
            if let Some(instant) = not_before {
                tokio::time::sleep_until(instant).await;
            }
            VTT
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (url, handle)
}

fn scratch() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let directory = tempfile::tempdir().unwrap();
    let audio = directory.path().join("clip.mp3");
    std::fs::write(&audio, b"fake audio bytes").unwrap();
    let work = directory.path().join("work");
    std::fs::create_dir(&work).unwrap();
    (directory, audio, work)
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

async fn transcribe(url: &str, audio: &Path, destination: &Path) -> anyhow::Result<NodeOutput> {
    let config = serde_json::json!({
        "path": audio,
        "provider": "openai_compatible",
        "base_url": url,
        "api_key": "k",
        "output_file": destination
    });
    NodeRegistry::with_builtins()
        .get("transcribe")
        .unwrap()
        .execute(&config, &Context::new())
        .await
}

#[cfg(unix)]
#[tokio::test]
async fn output_file_symlink_is_refused_and_its_target_is_untouched() {
    use std::os::unix::fs::symlink;

    for target_exists in [true, false] {
        let (url, provider) = start_provider(None).await;
        let (_directory, audio, work) = scratch();
        let target = work.join("target.txt");
        let destination = work.join("clip.vtt");
        if target_exists {
            std::fs::write(&target, ORIGINAL).unwrap();
        }
        symlink("target.txt", &destination).unwrap();

        let error = transcribe(&url, &audio, &destination)
            .await
            .expect_err("a symlinked output_file must be refused");

        let error = format!("{error:#}");
        assert!(error.contains("transcribe"), "{error}");
        assert!(error.contains("symlink"), "{error}");
        if target_exists {
            assert_eq!(std::fs::read(&target).unwrap(), ORIGINAL);
        } else {
            assert!(!target.exists(), "a dangling link was followed");
        }
        assert!(
            std::fs::symlink_metadata(&destination)
                .unwrap()
                .file_type()
                .is_symlink(),
            "the link itself must survive"
        );
        assert_eq!(staged_leftovers(&work), 0);
        provider.abort();
    }
}

#[cfg(unix)]
#[tokio::test]
async fn output_file_fifo_is_refused_without_waiting_for_a_reader() {
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::OpenOptionsExt;

    let (url, provider) = start_provider(None).await;
    let (_directory, audio, work) = scratch();
    let destination = work.join("clip.vtt");
    let name = std::ffi::CString::new(destination.as_os_str().as_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);

    let outcome = tokio::time::timeout(
        Duration::from_secs(5),
        transcribe(&url, &audio, &destination),
    )
    .await;
    // A writer blocked in open(2) is released by this reader, so a regression
    // fails here instead of hanging the runtime on shutdown.
    let _reader = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(&destination)
        .unwrap();

    let error = outcome
        .expect("the write waited for a FIFO reader")
        .expect_err("a FIFO output_file must be refused");
    let error = format!("{error:#}");
    assert!(error.contains("regular file"), "{error}");
    assert_eq!(staged_leftovers(&work), 0);
    provider.abort();
}

#[tokio::test]
async fn a_step_past_its_deadline_does_not_replace_the_output_file() {
    // The provider answers only after the step deadline, so the write is the
    // first thing the node attempts once its time is up.
    let deadline = Instant::now() + Duration::from_millis(150);
    let (url, provider) = start_provider(Some(deadline + Duration::from_millis(50))).await;
    let (_directory, audio, work) = scratch();
    let destination = work.join("clip.vtt");
    std::fs::write(&destination, ORIGINAL).unwrap();

    let error = ironflow::util::execution::with_execution_deadline(
        Some(deadline),
        transcribe(&url, &audio, &destination),
    )
    .await
    .expect_err("an expired step must not publish its output file");

    assert!(
        format!("{error:#}").contains("deadline exceeded"),
        "{error:#}"
    );
    assert_eq!(std::fs::read(&destination).unwrap(), ORIGINAL);
    assert_eq!(std::fs::read_dir(&work).unwrap().count(), 1);
    provider.abort();
}

#[tokio::test]
async fn output_file_holds_the_provider_body_byte_for_byte_and_replaces_a_regular_file() {
    let (url, provider) = start_provider(None).await;
    let (_directory, audio, work) = scratch();
    let destination = work.join("clip.vtt");
    std::fs::write(&destination, ORIGINAL).unwrap();

    let output = transcribe(&url, &audio, &destination).await.unwrap();

    assert_eq!(std::fs::read(&destination).unwrap(), VTT.as_bytes());
    assert_eq!(output["transcript_path"], destination.to_str().unwrap());
    assert_eq!(output["transcript"], VTT);
    assert_eq!(std::fs::read_dir(&work).unwrap().count(), 1);
    provider.abort();
}

#[tokio::test]
async fn a_missing_parent_directory_is_still_an_error_and_nothing_is_created() {
    let (url, provider) = start_provider(None).await;
    let (_directory, audio, work) = scratch();
    let destination = work.join("missing/deeper/clip.vtt");

    let error = transcribe(&url, &audio, &destination)
        .await
        .expect_err("transcribe does not create output directories");

    assert!(format!("{error:#}").contains("transcribe"), "{error:#}");
    assert!(!work.join("missing").exists());
    assert_eq!(std::fs::read_dir(&work).unwrap().count(), 0);
    provider.abort();
}

/// IF-185: the published transcript has the mode a plain write would have
/// left: the platform default when new, the existing bits when replaced.
#[cfg(unix)]
#[tokio::test]
async fn output_file_gets_the_default_mode_and_a_replaced_file_keeps_its_mode() {
    use std::os::unix::fs::PermissionsExt;

    fn mode(path: &Path) -> u32 {
        std::fs::metadata(path).unwrap().permissions().mode() & 0o7777
    }

    let (url, provider) = start_provider(None).await;
    let (_directory, audio, work) = scratch();
    // The mode a plain create leaves here under this process's umask.
    let plain = work.join("plain-create-probe");
    std::fs::File::create(&plain).unwrap();
    let expected = mode(&plain);
    std::fs::remove_file(&plain).unwrap();
    let created = work.join("created.vtt");
    let replaced = work.join("replaced.vtt");
    std::fs::write(&replaced, ORIGINAL).unwrap();
    std::fs::set_permissions(&replaced, std::fs::Permissions::from_mode(0o640)).unwrap();

    transcribe(&url, &audio, &created).await.unwrap();
    transcribe(&url, &audio, &replaced).await.unwrap();

    assert_eq!(
        format!("{:o}", mode(&created)),
        format!("{expected:o}"),
        "a new transcript must have the mode of a plain create"
    );
    assert_eq!(std::fs::read(&replaced).unwrap(), VTT.as_bytes());
    assert_eq!(
        format!("{:o}", mode(&replaced)),
        "640",
        "a replaced transcript must keep its permission bits"
    );
    assert_eq!(staged_leftovers(&work), 0);
    provider.abort();
}
