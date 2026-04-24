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
    pub runs_status_started_idx: String,
    pub tasks_run_id_idx: String,
}

#[derive(Debug, Clone)]
pub struct SqlEventTableNames {
    pub events: String,
    pub events_run_time_idx: String,
}

impl SqlStateTableNames {
    pub fn new(prefix: Option<&str>) -> Result<Self> {
        let prefix = normalized_prefix(prefix);
        let names = Self {
            runs: format!("{prefix}runs"),
            tasks: format!("{prefix}tasks"),
            runs_status_started_idx: format!("{prefix}runs_status_started_idx"),
            tasks_run_id_idx: format!("{prefix}tasks_run_id_idx"),
        };
        validate_identifier(&names.runs)?;
        validate_identifier(&names.tasks)?;
        validate_identifier(&names.runs_status_started_idx)?;
        validate_identifier(&names.tasks_run_id_idx)?;
        Ok(names)
    }
}

impl SqlEventTableNames {
    pub fn new(prefix: Option<&str>) -> Result<Self> {
        let prefix = normalized_prefix(prefix);
        let names = Self {
            events: format!("{prefix}events"),
            events_run_time_idx: format!("{prefix}events_run_time_idx"),
        };
        validate_identifier(&names.events)?;
        validate_identifier(&names.events_run_time_idx)?;
        Ok(names)
    }
}

fn normalized_prefix(prefix: Option<&str>) -> String {
    prefix
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_SQL_TABLE_PREFIX)
        .to_string()
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
    use super::{SqlDialect, SqlStateTableNames, validate_identifier};

    #[test]
    fn default_prefix_preserves_current_table_names() {
        let names = SqlStateTableNames::new(None).unwrap();
        assert_eq!(names.runs, "ironflow_runs");
        assert_eq!(names.tasks, "ironflow_tasks");
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
}
