use anyhow::Result;

use super::SqlStateStore;

impl SqlStateStore {
    pub(super) async fn ensure_schema(&self) -> Result<()> {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            r#"
            CREATE TABLE IF NOT EXISTS {} (
                id TEXT PRIMARY KEY,
                flow_name TEXT NOT NULL,
                status TEXT NOT NULL,
                started TEXT,
                finished TEXT,
                ctx TEXT NOT NULL
            )
            "#,
            self.tables.runs
        )))
        .execute(&self.pool)
        .await?;

        sqlx::query(sqlx::AssertSqlSafe(format!(
            r#"
            CREATE TABLE IF NOT EXISTS {} (
                run_id TEXT NOT NULL,
                name TEXT NOT NULL,
                node_type TEXT NOT NULL,
                status TEXT NOT NULL,
                attempt INTEGER NOT NULL,
                input TEXT,
                output TEXT,
                error TEXT,
                started TEXT,
                finished TEXT,
                PRIMARY KEY (run_id, name)
            )
            "#,
            self.tables.tasks
        )))
        .execute(&self.pool)
        .await?;

        sqlx::query(sqlx::AssertSqlSafe(format!(
            "CREATE INDEX IF NOT EXISTS {} ON {}(status, started)",
            self.tables.runs_status_started_idx, self.tables.runs
        )))
        .execute(&self.pool)
        .await?;

        sqlx::query(sqlx::AssertSqlSafe(format!(
            "CREATE INDEX IF NOT EXISTS {} ON {}(run_id)",
            self.tables.tasks_run_id_idx, self.tables.tasks
        )))
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}
