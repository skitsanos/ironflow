use super::*;

#[tokio::test]
async fn postgres_labeled_sqlite_urls_are_rejected_in_cli() {
    let directory = tempfile::tempdir().unwrap();
    let result = output(
        command(directory.path())
            .arg("list")
            .env("IRONFLOW_STORE", "postgres")
            .env("IRONFLOW_STORE_URL", "sqlite:if123-secret.sqlite?mode=rwc"),
    )
    .await;
    rejected(result, "state store", "if123-secret");
    assert!(!directory.path().join("if123-secret.sqlite").exists());
}

#[tokio::test]
async fn serve_rejects_each_disguised_postgres_store_before_any_connection() {
    for replica in [false, true] {
        for state in [false, true] {
            for env in [false, true] {
                let directory = tempfile::tempdir().unwrap();
                let trap = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
                trap.set_nonblocking(true).unwrap();
                let postgres = format!("postgres://if123-secret@{}/db", trap.local_addr().unwrap());
                let sqlite = "sqlite:if123-secret.sqlite?mode=rwc";
                let (state_url, event_url) = if state {
                    (sqlite, postgres.as_str())
                } else {
                    (postgres.as_str(), sqlite)
                };
                let mut cmd = command(directory.path());
                cmd.args(["serve", "--port", "0"]);
                if env {
                    cmd.env("IRONFLOW_REPLICA_MODE", replica.to_string())
                        .env("IRONFLOW_STORE", "postgres")
                        .env("IRONFLOW_STORE_URL", state_url)
                        .env("IRONFLOW_EVENT_STORE", "postgres")
                        .env("IRONFLOW_EVENT_STORE_URL", event_url);
                } else {
                    fs::write(directory.path().join("ironflow.yaml"), format!(
                        "replica_mode: {replica}\nstore_backend: postgres\nevent_store: postgres\nstore_url: {state_url:?}\nevent_store_url: {event_url:?}\n"
                    )).unwrap();
                }
                rejected(
                    output(&mut cmd).await,
                    if state { "state store" } else { "event store" },
                    "if123-secret",
                );
                assert!(!directory.path().join("if123-secret.sqlite").exists());
                assert_eq!(
                    trap.accept().unwrap_err().kind(),
                    std::io::ErrorKind::WouldBlock
                );
            }
        }
    }
}

#[tokio::test]
async fn replica_mode_does_not_admit_two_sqlite_urls_labeled_postgres() {
    let directory = tempfile::tempdir().unwrap();
    let result = output(
        command(directory.path())
            .args(["serve", "--host", "127.0.0.1", "--port", "0"])
            .env("IRONFLOW_REPLICA_MODE", "true")
            .env("IRONFLOW_STORE", "postgres")
            .env("IRONFLOW_STORE_URL", "sqlite:state.sqlite?mode=rwc")
            .env("IRONFLOW_EVENT_STORE", "postgres")
            .env("IRONFLOW_EVENT_STORE_URL", "sqlite:events.sqlite?mode=rwc"),
    )
    .await;
    rejected(result, "state store", "state.sqlite");
    assert!(!directory.path().join("state.sqlite").exists());
    assert!(!directory.path().join("events.sqlite").exists());
}

#[tokio::test]
async fn genuine_postgres_replicas_share_state_and_events() {
    let required = std::env::var("IRONFLOW_POSTGRES_TEST_REQUIRED").is_ok_and(|v| v == "1");
    let Some(url) = std::env::var("DATABASE_URL")
        .ok()
        .filter(|url| url.starts_with("postgres://") || url.starts_with("postgresql://"))
    else {
        assert!(!required, "live PostgreSQL test requires DATABASE_URL");
        eprintln!("Skipping live PostgreSQL replica test: DATABASE_URL is not configured");
        return;
    };
    let id = uuid::Uuid::new_v4().simple().to_string();
    let prefix = format!("if123_{}_", &id[..8]);
    let directory_a = tempfile::tempdir().unwrap();
    let directory_b = tempfile::tempdir().unwrap();
    let mut a = command(directory_a.path());
    let mut b = command(directory_b.path());
    let alias = if let Some(tail) = url.strip_prefix("postgres://") {
        format!("postgresql://{tail}")
    } else {
        url.clone()
    };
    for (cmd, url) in [(&mut a, &url), (&mut b, &alias)] {
        cmd.env("IRONFLOW_REPLICA_MODE", "true")
            .env("IRONFLOW_STORE", "postgres")
            .env("IRONFLOW_STORE_URL", url)
            .env("IRONFLOW_EVENT_STORE", "postgres")
            .env("IRONFLOW_EVENT_STORE_URL", url)
            .env("IRONFLOW_SQL_TABLE_PREFIX", &prefix);
    }
    let a = Server::start(a).await;
    let b = Server::start(b).await;
    let from_a = a.run_flow().await;
    b.assert_run(&from_a).await;
    let from_b = b.run_flow().await;
    a.assert_run(&from_b).await;
    a.stop().await;
    b.stop().await;
    sqlx::any::install_default_drivers();
    let pool = sqlx::AnyPool::connect(&url).await.unwrap();
    for id in [from_a, from_b] {
        let sql = format!("SELECT COUNT(*) FROM {prefix}events WHERE run_id = $1");
        let count: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert!(count > 0, "replica workflow did not persist events");
    }
    for suffix in [
        "event_deletions",
        "event_sequences",
        "events",
        "tasks",
        "run_leases",
        "schedule_claims",
        "runs",
    ] {
        let sql = format!("DROP TABLE {prefix}{suffix} CASCADE");
        sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .execute(&pool)
            .await
            .unwrap();
    }
    pool.close().await;
    assert!(!directory_a.path().join("data").exists());
    assert!(!directory_b.path().join("data").exists());
}
