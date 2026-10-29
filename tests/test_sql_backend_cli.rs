use std::fs;

#[cfg(feature = "postgres")]
#[path = "support/sql_backend_postgres.rs"]
mod postgres;
#[path = "support/sql_backend_cli.rs"]
mod support;

use support::{Server, command, output, rejected};

async fn startup_failure(cmd: tokio::process::Command) -> String {
    let failure = tokio::spawn(async move { Server::start(cmd).await.stop().await })
        .await
        .expect_err("invalid configuration must fail startup");
    let panic = failure.into_panic();
    panic
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| panic.downcast_ref::<&str>().map(|text| text.to_string()))
        .expect("startup panic must contain a diagnostic")
}

/// IF-163: SQLite is the default backend. Without `IRONFLOW_STORE`, a run
/// creates `ironflow.sqlite` under an owner-only store directory, writes no
/// JSON records, and `list` reads the run back from that file.
#[tokio::test]
async fn default_backend_is_sqlite_under_a_private_store_directory() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("probe.lua"), support::FLOW).unwrap();
    let store = directory.path().join("state").join("runs");

    let run = output(
        command(directory.path())
            .args(["run", "probe.lua", "--store-dir"])
            .arg(&store)
            .env("RUST_LOG", "info"),
    )
    .await;
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(run.status.success(), "run failed: {stderr}");
    assert!(
        stderr.contains("Using SQLite state store"),
        "default backend was not SQLite: {stderr}"
    );
    assert!(store.join("ironflow.sqlite").is_file());
    let json_records = fs::read_dir(&store)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "json"))
        .count();
    assert_eq!(
        json_records, 0,
        "the default store must not write JSON records"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = fs::metadata(&store).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700, "store directory must be owner-only");
    }

    let listed = output(
        command(directory.path())
            .args(["list", "--format", "json", "--store-dir"])
            .arg(&store),
    )
    .await;
    assert!(
        listed.status.success(),
        "list failed: {}",
        String::from_utf8_lossy(&listed.stderr)
    );
    let page: serde_json::Value = serde_json::from_slice(&listed.stdout).unwrap();
    let runs = page["runs"].as_array().unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0]["flow_name"], "sql-backend-probe");
    assert_eq!(runs[0]["status"], "success");
}

/// A symbolic link in place of the default store directory is refused rather
/// than followed, matching the JSON store's root rule.
#[cfg(unix)]
#[tokio::test]
async fn default_store_directory_must_not_be_a_symlink() {
    let directory = tempfile::tempdir().unwrap();
    let elsewhere = directory.path().join("elsewhere");
    fs::create_dir(&elsewhere).unwrap();
    std::os::unix::fs::symlink(&elsewhere, directory.path().join("runs")).unwrap();

    let listed =
        output(command(directory.path()).args(["list", "--format", "json", "--store-dir", "runs"]))
            .await;
    assert!(!listed.status.success());
    assert!(
        String::from_utf8_lossy(&listed.stderr).contains("must be a real directory"),
        "unexpected stderr: {}",
        String::from_utf8_lossy(&listed.stderr)
    );
    assert!(fs::read_dir(&elsewhere).unwrap().next().is_none());
}

#[tokio::test]
async fn sqlite_startup_failure_retains_the_cause_and_exit_code() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("not-a-directory"), "fixture").unwrap();
    let mut cmd = command(directory.path());
    cmd.env("IRONFLOW_STORE", "sqlite").env(
        "IRONFLOW_STORE_URL",
        "sqlite:not-a-directory/state.db?mode=rwc",
    );
    let diagnostic = startup_failure(cmd).await;
    assert!(
        diagnostic.contains("Failed to connect SQL state store"),
        "{diagnostic}"
    );
    assert!(diagnostic.contains("exit code 1"), "{diagnostic}");
}

#[tokio::test]
async fn startup_failure_diagnostics_do_not_expose_url_credentials() {
    let directory = tempfile::tempdir().unwrap();
    let mut cmd = command(directory.path());
    cmd.env("IRONFLOW_STORE", "sqlite").env(
        "IRONFLOW_STORE_URL",
        "postgres://fixture:startup-secret@127.0.0.1:1/db?token=startup-secret",
    );
    let diagnostic = startup_failure(cmd).await;
    assert!(
        diagnostic.contains("sqlite state store requires"),
        "{diagnostic}"
    );
    assert!(!diagnostic.contains("startup-secret"), "{diagnostic}");
    assert!(!directory.path().join("data").exists());
}

#[tokio::test]
async fn sqlite_state_commands_reject_postgres_urls_before_connecting() {
    for args in [
        vec!["run", "missing.lua"],
        vec!["list", "--format", "json"],
        vec!["inspect", "missing"],
        vec![
            "artifacts",
            "prune",
            "--before",
            "2026-09-01T00:00:00Z",
            "--confirm-offline",
        ],
    ] {
        let directory = tempfile::tempdir().unwrap();
        let trap = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        trap.set_nonblocking(true).unwrap();
        let url = format!(
            "postgres://user:if123-secret@{}/db?password=if123-secret",
            trap.local_addr().unwrap()
        );
        let result = output(
            command(directory.path())
                .args(args)
                .env("IRONFLOW_STORE", "sqlite")
                .env("IRONFLOW_STORE_URL", &url),
        )
        .await;
        rejected(result, "state store", "if123-secret");
        assert_eq!(
            trap.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
        assert!(!directory.path().join("data").exists());
    }
}

#[tokio::test]
async fn serve_preflights_event_url_before_creating_default_sqlite_state() {
    for url in [
        "postgres://user:if123-secret@127.0.0.1:1/db",
        "mysql://if123-secret",
        "",
        "not-a-url-if123-secret",
    ] {
        let directory = tempfile::tempdir().unwrap();
        let result = output(
            command(directory.path())
                .args(["serve", "--port", "0"])
                .env("IRONFLOW_STORE", "sqlite")
                .env("IRONFLOW_EVENT_STORE", "sqlite")
                .env("IRONFLOW_EVENT_STORE_URL", url),
        )
        .await;
        rejected(result, "event store", "if123-secret");
        assert!(
            !directory.path().join("data").exists(),
            "state directory was created before event validation"
        );
    }
}

#[tokio::test]
async fn invalid_url_environment_and_dotenv_override_valid_yaml() {
    for dotenv in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        fs::write(
            directory.path().join("ironflow.yaml"),
            "store_backend: sqlite\nstore_url: 'sqlite:valid.sqlite?mode=rwc'\n",
        )
        .unwrap();
        let mut cmd = command(directory.path());
        cmd.arg("list");
        if dotenv {
            fs::write(
                directory.path().join("selected.env"),
                "IRONFLOW_STORE_URL=mysql://if123-secret\n",
            )
            .unwrap();
            cmd.args(["--dotenv", "selected.env"]);
        } else {
            cmd.env("IRONFLOW_STORE_URL", "");
        }
        rejected(output(&mut cmd).await, "state store", "if123-secret");
        assert!(!directory.path().join("valid.sqlite").exists());
    }
}

#[tokio::test]
async fn unused_sql_urls_do_not_affect_json_or_memory_backends() {
    let directory = tempfile::tempdir().unwrap();
    let mut cmd = command(directory.path());
    // The JSON store is selected explicitly: SQLite is the default (IF-163)
    // and would validate the SQL URL.
    cmd.env("IRONFLOW_STORE", "json")
        .env("IRONFLOW_STORE_URL", "mysql://if123-secret")
        .env("IRONFLOW_EVENT_STORE_URL", "mysql://if123-secret");
    let server = Server::start(cmd).await;
    let id = server.run_flow().await;
    server.assert_run(&id).await;
    server.stop().await;
}

#[tokio::test]
async fn standalone_sqlite_defaults_and_explicit_urls_serve_and_persist() {
    for explicit in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let mut cmd = command(directory.path());
        cmd.env("IRONFLOW_STORE", "sqlite")
            .env("IRONFLOW_EVENT_STORE", "sqlite");
        if explicit {
            fs::write(directory.path().join("ironflow.yaml"), "store_backend: postgres\nevent_store: postgres\nstore_url: 'sqlite:state.sqlite?mode=rwc'\nevent_store_url: 'sqlite://events.sqlite?mode=rwc'\n").unwrap();
        }
        let server = Server::start(cmd).await;
        let id = server.run_flow().await;
        server.assert_run(&id).await;
        server.stop().await;
        let state = if explicit {
            "state.sqlite"
        } else {
            "data/runs/ironflow.sqlite"
        };
        let events = if explicit {
            "events.sqlite"
        } else {
            "data/runs/ironflow-events.sqlite"
        };
        for (file, table) in [(state, "ironflow_runs"), (events, "ironflow_events")] {
            assert!(directory.path().join(file).is_file());
            sqlx::any::install_default_drivers();
            let pool = sqlx::AnyPool::connect(&format!(
                "sqlite://{}",
                directory.path().join(file).display()
            ))
            .await
            .unwrap();
            let sql = format!("SELECT COUNT(*) FROM {table}");
            let count: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(sql.as_str()))
                .fetch_one(&pool)
                .await
                .unwrap();
            assert!(count > 0, "{table} did not persist workflow data");
            pool.close().await;
        }
        let result = output(
            command(directory.path())
                .args(["inspect", &id])
                .env("IRONFLOW_STORE", "sqlite"),
        )
        .await;
        assert!(
            result.status.success(),
            "CLI could not inspect the persisted run"
        );
    }
}
