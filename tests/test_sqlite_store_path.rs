//! IF-199: the default SQLite store is opened by filesystem path, so every
//! valid directory name works and nothing in it is read as URL syntax.

use std::fs;
use std::path::Path;

// One copy of the CLI harness per test binary: Clippy rejects a second load.
use super::test_sql_backend_cli::support::{self, Server, command, output, rejected};

/// Directory names that are ordinary on a filesystem and meaningful in a URL.
/// `file:x` matters because SQLite itself reads a leading `file:` as a URI.
const NAMES: [&str; 7] = [
    "My Runs",
    "runs%41data",
    "a?b",
    "a#b",
    "50% done & more",
    "file:x",
    "données",
];

fn diagnostic(result: &std::process::Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    )
}

async fn run_probe(workdir: &Path, store: &Path) {
    let run = output(
        command(workdir)
            .args(["run", "probe.lua", "--store-dir"])
            .arg(store),
    )
    .await;
    assert!(
        run.status.success(),
        "run failed for {store:?}: {}",
        diagnostic(&run)
    );
}

async fn listed_run_ids(workdir: &Path, store: &Path) -> Vec<String> {
    let listed = output(
        command(workdir)
            .args(["list", "--format", "json", "--store-dir"])
            .arg(store),
    )
    .await;
    assert!(
        listed.status.success(),
        "list failed for {store:?}: {}",
        diagnostic(&listed)
    );
    let page: serde_json::Value = serde_json::from_slice(&listed.stdout).unwrap();
    page["runs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|run| run["id"].as_str().unwrap().to_string())
        .collect()
}

fn entries(directory: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// A run with a relative store directory writes the database into exactly
/// that directory, and `list` and `inspect` find the run again through the
/// absolute spelling of the same directory (and the other way round).
#[tokio::test]
async fn any_directory_name_holds_the_default_store_under_either_spelling() {
    for name in NAMES {
        for run_with_relative in [true, false] {
            let directory = tempfile::tempdir().unwrap();
            let root = directory.path().canonicalize().unwrap();
            fs::write(root.join("probe.lua"), support::FLOW).unwrap();
            let relative = Path::new(name).to_path_buf();
            let absolute = root.join(name);
            let (writer, reader) = if run_with_relative {
                (&relative, &absolute)
            } else {
                (&absolute, &relative)
            };

            run_probe(&root, writer).await;

            assert!(
                absolute.join("ironflow.sqlite").is_file(),
                "{name:?}: database is not in the requested directory; found {:?}",
                entries(&root)
            );
            assert_eq!(
                entries(&root),
                {
                    let mut expected = vec![name.to_string(), "probe.lua".to_string()];
                    expected.sort();
                    expected
                },
                "{name:?}: the run created something outside the store directory"
            );
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt as _;
                let mode = fs::metadata(&absolute).unwrap().permissions().mode() & 0o777;
                assert_eq!(mode, 0o700, "{name:?}: store directory must be owner-only");
            }

            let ids = listed_run_ids(&root, reader).await;
            assert_eq!(ids.len(), 1, "{name:?}: run not found via {reader:?}");
            let inspected = output(
                command(&root)
                    .args(["inspect", &ids[0], "--store-dir"])
                    .arg(reader),
            )
            .await;
            assert!(
                inspected.status.success() && diagnostic(&inspected).contains("sql-backend-probe"),
                "{name:?}: inspect via {reader:?} failed: {}",
                diagnostic(&inspected)
            );
        }
    }
}

/// `runs%41data` used to be percent-decoded to `runsAdata`. When that
/// directory exists the database must still not land in it.
#[tokio::test]
async fn percent_sequences_do_not_redirect_the_store_to_a_decoded_directory() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    fs::write(root.join("probe.lua"), support::FLOW).unwrap();
    fs::create_dir(root.join("runsAdata")).unwrap();

    run_probe(root, Path::new("runs%41data")).await;

    assert!(root.join("runs%41data").join("ironflow.sqlite").is_file());
    assert_eq!(entries(&root.join("runsAdata")), Vec::<String>::new());
}

/// The event store takes the same route as the state store, and a restarted
/// server finds the run that the first one stored.
#[tokio::test]
async fn serve_opens_state_and_event_stores_in_a_directory_with_url_syntax() {
    let directory = tempfile::tempdir().unwrap();
    let store = directory.path().join("My Runs #1?").join("50%41");
    let server = |store_dir: &str| {
        let mut cmd = command(directory.path());
        cmd.env("IRONFLOW_EVENT_STORE", "sqlite")
            .env("IRONFLOW_STORE_DIR", store_dir);
        Server::start(cmd)
    };

    let first = server("My Runs #1?/50%41").await;
    let id = first.run_flow().await;
    first.stop().await;

    assert!(store.join("ironflow.sqlite").is_file());
    assert!(store.join("ironflow-events.sqlite").is_file());
    assert_eq!(entries(directory.path()), vec!["My Runs #1?".to_string()]);

    let second = server(store.to_str().unwrap()).await;
    second.assert_run(&id).await;
    second.stop().await;
}

/// Each pooled connection would open its own empty in-memory (or temporary)
/// database, so these URLs are refused before anything is created.
#[tokio::test]
async fn in_memory_and_temporary_sqlite_urls_are_rejected_before_any_side_effect() {
    for url in [
        "sqlite::memory:",
        "sqlite://:memory:",
        "sqlite://?mode=memory",
        "sqlite:shared.db?mode=memory&cache=shared",
        "sqlite:file::memory:",
        "sqlite:%3Amemory%3A",
        "sqlite:",
        "sqlite://",
        "sqlite:?mode=rwc",
    ] {
        for args in [
            vec!["run", "probe.lua"],
            vec!["list", "--format", "json"],
            vec!["inspect", "missing"],
        ] {
            let directory = tempfile::tempdir().unwrap();
            fs::write(directory.path().join("probe.lua"), support::FLOW).unwrap();
            let result = output(
                command(directory.path())
                    .args(args)
                    .env("IRONFLOW_STORE_URL", url),
            )
            .await;
            let text = diagnostic(&result);
            assert!(
                text.contains("in-memory") && text.contains("IRONFLOW_STORE_URL"),
                "{url}: unclear diagnostic: {text}"
            );
            assert!(!text.contains("duplicate column"), "{url}: {text}");
            rejected(result, "state store", "shared.db");
            assert_eq!(entries(directory.path()), vec!["probe.lua".to_string()]);
        }

        let directory = tempfile::tempdir().unwrap();
        let result = output(
            command(directory.path())
                .args(["serve", "--port", "0"])
                .env("IRONFLOW_EVENT_STORE", "sqlite")
                .env("IRONFLOW_EVENT_STORE_URL", url),
        )
        .await;
        let text = diagnostic(&result);
        assert!(
            text.contains("in-memory") && text.contains("IRONFLOW_EVENT_STORE_URL"),
            "{url}: unclear diagnostic: {text}"
        );
        rejected(result, "event store", "shared.db");
        assert!(
            entries(directory.path()).is_empty(),
            "{url}: state directory was created before event validation"
        );
    }
}

/// An explicit SQLite URL keeps its documented behaviour: IronFlow passes it
/// to the driver unchanged, creates no directory for it, and the URL's own
/// percent-encoding is decoded by the driver.
#[tokio::test]
async fn explicit_sqlite_urls_still_open_the_file_they_name() {
    for (url, file) in [
        (
            "sqlite://state/ironflow.sqlite?mode=rwc",
            "state/ironflow.sqlite",
        ),
        ("sqlite:state/plain.db?mode=rwc", "state/plain.db"),
        ("sqlite:state/my%20runs.db?mode=rwc", "state/my runs.db"),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        fs::write(root.join("probe.lua"), support::FLOW).unwrap();
        fs::create_dir(root.join("state")).unwrap();

        let run = output(
            command(root)
                .args(["run", "probe.lua"])
                .env("IRONFLOW_STORE_URL", url),
        )
        .await;
        assert!(run.status.success(), "{url}: {}", diagnostic(&run));
        assert!(root.join(file).is_file(), "{url}: {:?}", entries(root));
        assert!(
            !root.join("data").exists(),
            "{url}: the default store directory was created for an explicit URL"
        );

        let listed = output(
            command(root)
                .args(["list", "--format", "json"])
                .env("IRONFLOW_STORE_URL", url),
        )
        .await;
        assert!(listed.status.success(), "{url}: {}", diagnostic(&listed));
        let page: serde_json::Value = serde_json::from_slice(&listed.stdout).unwrap();
        assert_eq!(page["runs"].as_array().unwrap().len(), 1, "{url}");
    }
}

/// SQLite takes UTF-8 file names. A store directory that is not UTF-8 is
/// refused with its own message, and is not created first.
#[cfg(unix)]
#[tokio::test]
async fn a_store_directory_that_is_not_utf8_is_refused_without_being_created() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt as _;

    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("probe.lua"), support::FLOW).unwrap();
    let name = OsStr::from_bytes(b"runs-\xff");

    let result = output(
        command(directory.path())
            .args(["run", "probe.lua", "--store-dir"])
            .arg(name),
    )
    .await;

    assert!(!result.status.success());
    assert!(
        diagnostic(&result).contains("valid UTF-8"),
        "{}",
        diagnostic(&result)
    );
    assert_eq!(entries(directory.path()), vec!["probe.lua".to_string()]);
}

/// The stores refuse an in-memory URL themselves, for callers that do not go
/// through the CLI resolver.
#[tokio::test]
async fn sql_stores_refuse_in_memory_urls_at_the_library_boundary() {
    use ironflow::storage::event_store::SqlEventStore;
    use ironflow::storage::sql_store::SqlStateStore;

    for url in ["sqlite::memory:", "sqlite://?mode=memory", "sqlite:"] {
        let state = SqlStateStore::new(url).await.err().unwrap().to_string();
        assert!(state.contains("in-memory"), "{url}: {state}");
        let events = SqlEventStore::new(url).await.err().unwrap().to_string();
        assert!(events.contains("in-memory"), "{url}: {events}");
    }
}
