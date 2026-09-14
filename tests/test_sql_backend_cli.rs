use std::fs;

#[cfg(feature = "postgres")]
#[path = "support/sql_backend_postgres.rs"]
mod postgres;
#[path = "support/sql_backend_cli.rs"]
mod support;

use support::{Server, command, output, rejected};

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
    cmd.env("IRONFLOW_STORE_URL", "mysql://if123-secret")
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
