use super::SqlStateStore;
use crate::storage::run_listing::normalized_started;
use crate::storage::{RunListQuery, RunSummaryPage, StorageError, StorageResult};

impl SqlStateStore {
    pub(super) async fn page_run_summaries(
        &self,
        query: &RunListQuery,
    ) -> StorageResult<RunSummaryPage> {
        // A previous IronFlow version can still insert rows without the
        // derived ordering key after this process has completed its startup
        // migration. Repair those rows before applying a cursor so they do
        // not move from the NULL partition between pages.
        self.backfill_started_micros().await?;

        let mut parameter = 1;
        let mut sql = format!(
            "SELECT r.id, r.flow_name, r.status, r.started, r.finished, \
             (SELECT COUNT(*) FROM {} t WHERE t.run_id = r.id) AS task_count \
             FROM {} r WHERE 1 = 1",
            self.tables.tasks, self.tables.runs
        );

        let status = query.status().map(ToString::to_string);
        if status.is_some() {
            sql.push_str(&format!(" AND r.status = {}", self.placeholder(parameter)));
            parameter += 1;
        }

        let cursor_started = query
            .after()
            .and_then(|cursor| normalized_started(cursor.started()));
        let cursor_id = query.after().map(|cursor| cursor.id().to_string());
        if query.after().is_some() {
            if cursor_started.is_some() {
                let earlier = self.placeholder(parameter);
                parameter += 1;
                let equal = self.placeholder(parameter);
                parameter += 1;
                let lower_id = self.placeholder(parameter);
                parameter += 1;
                sql.push_str(&format!(
                    " AND (r.started_micros < {earlier} OR (r.started_micros = {equal} AND r.id < {lower_id}) OR r.started_micros IS NULL)"
                ));
            } else {
                sql.push_str(&format!(
                    " AND r.started_micros IS NULL AND r.id < {}",
                    self.placeholder(parameter)
                ));
                parameter += 1;
            }
        }

        sql.push_str(" ORDER BY r.started_micros DESC NULLS LAST, r.id DESC");
        sql.push_str(&format!(" LIMIT {}", self.placeholder(parameter)));
        self.execute_summary_page(sql, status, cursor_started, cursor_id, query)
            .await
    }

    async fn execute_summary_page(
        &self,
        sql: String,
        status: Option<String>,
        cursor_started: Option<i64>,
        cursor_id: Option<String>,
        query: &RunListQuery,
    ) -> StorageResult<RunSummaryPage> {
        let fetch_limit = query
            .limit()
            .get()
            .checked_add(1)
            .and_then(|value| i64::try_from(value).ok())
            .ok_or_else(|| StorageError::invalid_input("run-list page size is too large"))?;
        let mut statement = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()));
        if let Some(status) = status {
            statement = statement.bind(status);
        }
        if let Some(cursor_id) = cursor_id {
            if let Some(cursor_started) = cursor_started {
                statement = statement
                    .bind(cursor_started)
                    .bind(cursor_started)
                    .bind(cursor_id);
            } else {
                statement = statement.bind(cursor_id);
            }
        }
        let rows = statement
            .bind(fetch_limit)
            .fetch_all(&self.pool)
            .await
            .map_err(|error| StorageError::backend("Failed to page SQL run summaries", error))?;
        let summaries = rows
            .iter()
            .map(Self::row_to_summary)
            .collect::<StorageResult<Vec<_>>>()?;
        Ok(RunSummaryPage::from_ordered(summaries, query))
    }
}
