#![cfg(feature = "postgres")]

use anyhow::Context as _;
use ironflow::storage::sql_store::SqlStateStore;
use uuid::Uuid;

/// Isolated PostgreSQL state-store schema for one integration test.
pub struct PostgresStateTest {
    url: String,
    prefix: String,
}

impl PostgresStateTest {
    pub fn from_env(label: &str) -> Option<Self> {
        dotenvy::dotenv().ok();
        let required = std::env::var("IRONFLOW_POSTGRES_TEST_REQUIRED")
            .is_ok_and(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"));
        let url = std::env::var("DATABASE_URL")
            .ok()
            .filter(|url| url.starts_with("postgres://") || url.starts_with("postgresql://"));
        let Some(url) = url else {
            if required {
                panic!("PostgreSQL integration tests require a PostgreSQL DATABASE_URL");
            }
            eprintln!("Skipping PostgreSQL test: DATABASE_URL is not configured for PostgreSQL");
            return None;
        };
        let id = Uuid::new_v4().simple().to_string();
        Some(Self {
            url,
            prefix: format!("{label}_{}_", &id[..8]),
        })
    }

    pub async fn state_store(&self) -> anyhow::Result<SqlStateStore> {
        SqlStateStore::new_with_prefix(&self.url, Some(&self.prefix))
            .await
            .context("PostgreSQL state test store should connect")
    }

    #[allow(dead_code)]
    pub fn url(&self) -> &str {
        &self.url
    }

    #[allow(dead_code)]
    pub fn table(&self, suffix: &str) -> String {
        format!("{}{suffix}", self.prefix)
    }

    pub async fn cleanup(&self) -> anyhow::Result<()> {
        sqlx::any::install_default_drivers();
        let pool = sqlx::AnyPool::connect(&self.url)
            .await
            .context("connect for PostgreSQL test cleanup")?;
        for suffix in ["tasks", "run_leases", "schedule_claims", "runs"] {
            let sql = format!("DROP TABLE IF EXISTS {}{suffix} CASCADE", self.prefix);
            sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
                .execute(&pool)
                .await
                .with_context(|| format!("drop PostgreSQL test table {}{suffix}", self.prefix))?;
        }
        pool.close().await;
        Ok(())
    }
}
