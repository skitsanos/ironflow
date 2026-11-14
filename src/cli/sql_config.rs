use std::path::Path;

use anyhow::{Context as _, Result};

use crate::storage::sql_names::{SqlDialect, SqlEventTableNames, SqlStateTableNames};
use crate::storage::sqlite_url;
use crate::util::sensitive_url::Connection;

use super::{IronFlowConfig, resolution::environment_string, sqlite_path};

/// A SQL store URL and a description of where it points that is safe to log.
pub(super) struct SqlTarget {
    pub(super) url: String,
    pub(super) location: String,
}

#[derive(Clone, Copy)]
pub(super) enum SqlStoreKind {
    State,
    Event,
}

impl SqlStoreKind {
    fn names(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Self::State => ("state store", "IRONFLOW_STORE_URL", "store_url"),
            Self::Event => ("event store", "IRONFLOW_EVENT_STORE_URL", "event_store_url"),
        }
    }
}

// Resolve only selected SQL backends; unrelated URL settings remain unused.
// Everything that can be rejected from configuration alone is rejected here,
// because `serve` calls this for both stores before either may create files.
pub(super) fn configured_url(
    cfg: &IronFlowConfig,
    backend: &str,
    kind: SqlStoreKind,
) -> Result<Option<String>> {
    if !matches!(backend, "sqlite" | "postgres") {
        return Ok(None);
    }
    let (_, environment, _) = kind.names();
    let fallback = match kind {
        SqlStoreKind::State => &cfg.store_url,
        SqlStoreKind::Event => &cfg.event_store_url,
    };
    let url = environment_string(environment)?.or_else(|| fallback.clone());
    validate_url(backend, kind, url.as_deref())?;
    validate_table_prefix(cfg, kind)?;
    Ok(url)
}

fn validate_table_prefix(cfg: &IronFlowConfig, kind: SqlStoreKind) -> Result<()> {
    let prefix = super::store_factory::resolve_sql_table_prefix(cfg)?;
    let (label, _, _) = kind.names();
    match kind {
        SqlStoreKind::State => SqlStateTableNames::new(prefix.as_deref()).map(drop),
        SqlStoreKind::Event => SqlEventTableNames::new(prefix.as_deref()).map(drop),
    }
    .map_err(|error| anyhow::anyhow!("Invalid SQL {label} table prefix: {error}"))
}

fn validate_url(backend: &str, kind: SqlStoreKind, url: Option<&str>) -> Result<()> {
    let (label, environment, field) = kind.names();
    let (dialect, scheme) = match backend {
        "sqlite" => (SqlDialect::Sqlite, "sqlite:"),
        "postgres" => (SqlDialect::Postgres, "postgres:// or postgresql://"),
        _ => anyhow::bail!("Unsupported SQL backend"),
    };
    let Some(url) = url else {
        if dialect == SqlDialect::Postgres {
            anyhow::bail!("Postgres {label} requires {environment} or {field} in config");
        }
        return Ok(());
    };
    if SqlDialect::from_url(url).ok() != Some(dialect) {
        // Never include a raw URL or its parsing error in a configuration diagnostic.
        anyhow::bail!(
            "{backend} {label} requires a {scheme} URL in {environment} or {field} in config"
        );
    }
    if dialect == SqlDialect::Sqlite && sqlite_url::is_ephemeral(url) {
        anyhow::bail!(
            "{backend} {label} requires a {scheme} URL that names a database file in \
             {environment} or {field} in config; {}",
            sqlite_url::EPHEMERAL_REJECTION
        );
    }
    Ok(())
}

/// Resolve the URL of a selected SQL store. A configured URL is passed to the
/// driver unchanged. Without one, SQLite uses a file under `store_dir`, which
/// is created only after its URL is known to be usable.
pub(super) fn resolve_url(
    cfg: &IronFlowConfig,
    store_dir: &Path,
    backend: &str,
    kind: SqlStoreKind,
) -> Result<SqlTarget> {
    if let Some(url) = configured_url(cfg, backend, kind)? {
        let location = Connection::new(&url).to_string();
        return Ok(SqlTarget { url, location });
    }
    if backend != "sqlite" {
        anyhow::bail!("Unsupported SQL backend");
    }
    let filename = match kind {
        SqlStoreKind::State => "ironflow.sqlite",
        SqlStoreKind::Event => "ironflow-events.sqlite",
    };
    let file = sqlite_path::database_file(store_dir, filename)?;
    let url = sqlite_path::create_url(&file)?;
    create_private_store_dir(store_dir)?;
    Ok(SqlTarget {
        url,
        location: file.display().to_string(),
    })
}

/// Create the directory that holds the default SQLite files. Run records carry
/// workflow context, so the directory is owner-only on Unix, like the JSON
/// store's root, and must be a real directory rather than a symbolic link.
fn create_private_store_dir(store_dir: &Path) -> Result<()> {
    std::fs::create_dir_all(store_dir)
        .with_context(|| format!("Failed to create store dir: {}", store_dir.display()))?;
    let metadata = std::fs::symlink_metadata(store_dir)
        .with_context(|| format!("Failed to inspect store dir: {}", store_dir.display()))?;
    if !metadata.is_dir() {
        anyhow::bail!(
            "Store dir must be a real directory, not a symbolic link or file: {}",
            store_dir.display()
        );
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(store_dir, std::fs::Permissions::from_mode(0o700))
            .with_context(|| format!("Failed to restrict store dir: {}", store_dir.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{SqlStoreKind, validate_url};

    #[test]
    fn supported_schemes_match_storage_dialect_detection() {
        for kind in [SqlStoreKind::State, SqlStoreKind::Event] {
            for url in ["sqlite:relative.db", "sqlite://absolute.db?mode=rwc"] {
                validate_url("sqlite", kind, Some(url)).unwrap();
            }
            for url in ["postgres://user@host/db", "postgresql://user@host/db"] {
                validate_url("postgres", kind, Some(url)).unwrap();
            }
        }
    }

    #[test]
    fn mismatched_and_unsupported_schemes_are_secret_safe() {
        for kind in [SqlStoreKind::State, SqlStoreKind::Event] {
            for backend in ["sqlite", "postgres"] {
                let mismatch = if backend == "sqlite" {
                    "postgres://user:secret-sentinel@host/db"
                } else {
                    "sqlite:secret-sentinel.db"
                };
                for url in [
                    mismatch,
                    "",
                    "mysql://secret-sentinel",
                    "postgres-secret-sentinel://host",
                    " secret-sentinel",
                    "POSTGRES://secret-sentinel",
                ] {
                    let error =
                        format!("{:#}", validate_url(backend, kind, Some(url)).unwrap_err());
                    let (label, environment, field) = kind.names();
                    assert!(
                        error.contains(label)
                            && error.contains(environment)
                            && error.contains(field)
                    );
                    assert!(!error.contains("secret-sentinel"));
                }
            }
        }
    }

    #[test]
    fn sqlite_urls_without_a_database_file_are_rejected_without_echoing_them() {
        for kind in [SqlStoreKind::State, SqlStoreKind::Event] {
            for url in [
                "sqlite::memory:",
                "sqlite://?mode=memory",
                "sqlite:secret-sentinel.db?mode=memory",
                "sqlite:",
            ] {
                let error = validate_url("sqlite", kind, Some(url))
                    .unwrap_err()
                    .to_string();
                let (label, environment, field) = kind.names();
                assert!(
                    error.contains(label) && error.contains(environment) && error.contains(field)
                );
                assert!(error.contains("in-memory"), "{error}");
                assert!(!error.contains("secret-sentinel"));
            }
        }
    }

    #[test]
    fn only_sqlite_may_omit_its_url() {
        for kind in [SqlStoreKind::State, SqlStoreKind::Event] {
            validate_url("sqlite", kind, None).unwrap();
            let error = validate_url("postgres", kind, None)
                .unwrap_err()
                .to_string();
            assert!(error.contains(kind.names().1));
        }
    }
}
