//! Concurrent schema creation (IF-055).
//!
//! `CREATE TABLE IF NOT EXISTS` is not atomic on Postgres: the existence check
//! and the create are separate steps, so two processes starting against a fresh
//! database can both observe "absent" and one then fails against the catalog's
//! unique indexes. Two Kubernetes replicas hit this reliably, crash-looping one
//! pod. Losing the race must be treated as success.
//!
//! Gated on DATABASE_URL like the other Postgres suites; skipped otherwise.

#![cfg(feature = "postgres")]

use ironflow::storage::event_store::SqlEventStore;
use ironflow::storage::sql_store::SqlStateStore;

fn postgres_database_url() -> Option<String> {
    dotenvy::dotenv().ok();
    std::env::var("DATABASE_URL")
        .ok()
        .filter(|url| url.starts_with("postgres://") || url.starts_with("postgresql://"))
}

fn unique_sql_prefix(label: &str) -> String {
    let id = uuid::Uuid::new_v4().simple().to_string();
    format!("{}_{}_", label, &id[..8])
}

async fn drop_tables(url: &str, prefix: &str) {
    sqlx::any::install_default_drivers();
    if let Ok(pool) = sqlx::AnyPool::connect(url).await {
        for table in [
            "events",
            "event_deletions",
            "event_sequences",
            "tasks",
            "runs",
        ] {
            let _ = sqlx::query(sqlx::AssertSqlSafe(
                format!("DROP TABLE IF EXISTS {prefix}{table} CASCADE").as_str(),
            ))
            .execute(&pool)
            .await;
        }
    }
}

/// Eight concurrent state-store constructions against an empty schema. Before
/// the fix a single concurrent pair failed every time.
#[tokio::test]
async fn concurrent_state_store_schema_creation_all_succeed() {
    let Some(url) = postgres_database_url() else {
        eprintln!("Skipping test: DATABASE_URL is not configured for Postgres");
        return;
    };
    let prefix = unique_sql_prefix("pg_ddl_state");
    drop_tables(&url, &prefix).await;

    let mut handles = Vec::new();
    for _ in 0..8 {
        let url = url.clone();
        let prefix = prefix.clone();
        handles.push(tokio::spawn(async move {
            SqlStateStore::new_with_prefix(&url, Some(&prefix))
                .await
                .map(|_| ())
        }));
    }

    let mut failures = Vec::new();
    for handle in handles {
        if let Err(error) = handle.await.expect("task panicked") {
            failures.push(error.to_string());
        }
    }
    drop_tables(&url, &prefix).await;
    assert!(
        failures.is_empty(),
        "concurrent schema creation failed: {failures:#?}"
    );
}

/// The event store creates more objects, including a unique index, and was the
/// second failure surfaced once the tables themselves were fixed.
#[tokio::test]
async fn concurrent_event_store_schema_creation_all_succeed() {
    let Some(url) = postgres_database_url() else {
        eprintln!("Skipping test: DATABASE_URL is not configured for Postgres");
        return;
    };
    let prefix = unique_sql_prefix("pg_ddl_events");
    drop_tables(&url, &prefix).await;

    let mut handles = Vec::new();
    for _ in 0..8 {
        let url = url.clone();
        let prefix = prefix.clone();
        handles.push(tokio::spawn(async move {
            SqlEventStore::new_with_prefix(&url, Some(&prefix))
                .await
                .map(|_| ())
        }));
    }

    let mut failures = Vec::new();
    for handle in handles {
        if let Err(error) = handle.await.expect("task panicked") {
            failures.push(error.to_string());
        }
    }
    drop_tables(&url, &prefix).await;
    assert!(
        failures.is_empty(),
        "concurrent event schema creation failed: {failures:#?}"
    );
}
