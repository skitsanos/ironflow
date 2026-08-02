use anyhow::Result;

const DEFAULT_SQL_TABLE_PREFIX: &str = "ironflow_";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SqlDialect {
    Sqlite,
    Postgres,
}

impl SqlDialect {
    pub fn from_url(url: &str) -> Result<Self> {
        if url.starts_with("sqlite:") {
            return Ok(Self::Sqlite);
        }
        if url.starts_with("postgres://") || url.starts_with("postgresql://") {
            return Ok(Self::Postgres);
        }
        anyhow::bail!("Unsupported SQL store URL scheme");
    }

    pub fn placeholder(self, index: usize) -> String {
        match self {
            Self::Sqlite => "?".to_string(),
            Self::Postgres => format!("${index}"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SqlStateTableNames {
    pub runs: String,
    pub tasks: String,
    pub runs_started_idx: String,
    pub runs_status_started_idx: String,
    pub tasks_run_id_idx: String,
    pub run_leases: String,
    pub run_leases_expiry_idx: String,
    pub schedule_claims: String,
    pub schedule_claims_cleanup_idx: String,
}

#[derive(Debug, Clone)]
pub struct SqlEventTableNames {
    pub events: String,
    pub event_sequences: String,
    pub event_deletions: String,
    pub events_run_time_idx: String,
    pub events_run_sequence_idx: String,
    pub events_null_sequence_idx: String,
}

impl SqlStateTableNames {
    pub fn new(prefix: Option<&str>) -> Result<Self> {
        let prefix = normalized_prefix(prefix);
        let names = Self {
            runs: format!("{prefix}runs"),
            tasks: format!("{prefix}tasks"),
            runs_started_idx: format!("{prefix}runs_started_idx"),
            runs_status_started_idx: format!("{prefix}runs_status_started_id_idx"),
            tasks_run_id_idx: format!("{prefix}tasks_run_id_idx"),
            run_leases: format!("{prefix}run_leases"),
            run_leases_expiry_idx: format!("{prefix}run_leases_expiry_idx"),
            schedule_claims: format!("{prefix}schedule_claims"),
            schedule_claims_cleanup_idx: format!("{prefix}schedule_claims_gc_idx"),
        };
        validate_identifier(&names.runs)?;
        validate_identifier(&names.tasks)?;
        validate_identifier(&names.runs_started_idx)?;
        validate_identifier(&names.runs_status_started_idx)?;
        validate_identifier(&names.tasks_run_id_idx)?;
        validate_identifier(&names.run_leases)?;
        validate_identifier(&names.run_leases_expiry_idx)?;
        validate_identifier(&names.schedule_claims)?;
        validate_identifier(&names.schedule_claims_cleanup_idx)?;
        Ok(names)
    }
}

impl SqlEventTableNames {
    pub fn new(prefix: Option<&str>) -> Result<Self> {
        let prefix = normalized_prefix(prefix);
        let names = Self {
            events: format!("{prefix}events"),
            event_sequences: format!("{prefix}event_sequences"),
            event_deletions: format!("{prefix}event_deletions"),
            events_run_time_idx: format!("{prefix}events_run_time_idx"),
            events_run_sequence_idx: format!("{prefix}events_run_seq_idx"),
            events_null_sequence_idx: format!("{prefix}events_null_seq_idx"),
        };
        validate_identifier(&names.events)?;
        validate_identifier(&names.event_sequences)?;
        validate_identifier(&names.event_deletions)?;
        validate_identifier(&names.events_run_time_idx)?;
        validate_identifier(&names.events_run_sequence_idx)?;
        validate_identifier(&names.events_null_sequence_idx)?;
        Ok(names)
    }
}

fn normalized_prefix(prefix: Option<&str>) -> String {
    prefix
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_SQL_TABLE_PREFIX)
        .to_ascii_lowercase()
}

fn validate_identifier(identifier: &str) -> Result<()> {
    if identifier.len() > 63 {
        anyhow::bail!(
            "SQL table prefix is too long; derived identifier '{}' exceeds 63 bytes",
            identifier
        );
    }

    let mut chars = identifier.chars();
    let Some(first) = chars.next() else {
        anyhow::bail!("SQL identifier cannot be empty");
    };

    if !(first == '_' || first.is_ascii_alphabetic()) {
        anyhow::bail!(
            "Invalid SQL table prefix; derived identifier '{}' must start with a letter or underscore",
            identifier
        );
    }

    if !chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric()) {
        anyhow::bail!(
            "Invalid SQL table prefix; derived identifier '{}' may contain only ASCII letters, digits, and underscores",
            identifier
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{SqlDialect, SqlEventTableNames, SqlStateTableNames, validate_identifier};

    #[test]
    fn default_prefix_preserves_current_table_names() {
        let names = SqlStateTableNames::new(None).unwrap();
        assert_eq!(names.runs, "ironflow_runs");
        assert_eq!(names.tasks, "ironflow_tasks");
        assert_eq!(names.run_leases, "ironflow_run_leases");
        assert_eq!(names.schedule_claims, "ironflow_schedule_claims");
        assert_eq!(
            names.schedule_claims_cleanup_idx,
            "ironflow_schedule_claims_gc_idx"
        );

        let events = SqlEventTableNames::new(None).unwrap();
        assert_eq!(events.events, "ironflow_events");
        assert_eq!(events.event_sequences, "ironflow_event_sequences");
        assert_eq!(events.event_deletions, "ironflow_event_deletions");
    }

    #[test]
    fn rejects_unsafe_identifier_fragments() {
        assert!(validate_identifier("bad-name").is_err());
        assert!(validate_identifier("bad.name").is_err());
        assert!(validate_identifier("1bad").is_err());
        assert!(validate_identifier("bad;drop").is_err());
    }

    #[test]
    fn dialect_placeholders_match_backend() {
        assert_eq!(SqlDialect::Sqlite.placeholder(1), "?");
        assert_eq!(SqlDialect::Postgres.placeholder(2), "$2");
    }

    #[test]
    fn normalizes_identifiers_to_postgres_unquoted_case() {
        let names = SqlStateTableNames::new(Some("Tenant_A_")).unwrap();
        assert_eq!(names.runs, "tenant_a_runs");
        assert_eq!(names.tasks, "tenant_a_tasks");
    }

    #[test]
    fn cleanup_index_does_not_reduce_the_existing_prefix_limit() {
        let prefix = "p".repeat(63 - "runs_status_started_id_idx".len());
        SqlStateTableNames::new(Some(&prefix)).unwrap();
    }
}
