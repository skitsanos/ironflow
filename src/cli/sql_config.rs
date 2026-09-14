use std::path::Path;

use anyhow::{Context as _, Result};

use crate::storage::sql_names::SqlDialect;

use super::{IronFlowConfig, resolution::environment_string};

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
    Ok(url)
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
    Ok(())
}

pub(super) fn resolve_url(
    cfg: &IronFlowConfig,
    store_dir: &Path,
    backend: &str,
    kind: SqlStoreKind,
) -> Result<String> {
    if let Some(url) = configured_url(cfg, backend, kind)? {
        return Ok(url);
    }
    if backend != "sqlite" {
        anyhow::bail!("Unsupported SQL backend");
    }
    std::fs::create_dir_all(store_dir)
        .with_context(|| format!("Failed to create store dir: {}", store_dir.display()))?;
    let filename = match kind {
        SqlStoreKind::State => "ironflow.sqlite",
        SqlStoreKind::Event => "ironflow-events.sqlite",
    };
    let path = store_dir.join(filename);
    Ok(format!("sqlite://{}?mode=rwc", path.to_string_lossy()))
}

#[cfg(test)]
mod tests {
    use super::{SqlStoreKind, validate_url};

    #[test]
    fn supported_schemes_match_storage_dialect_detection() {
        for kind in [SqlStoreKind::State, SqlStoreKind::Event] {
            for url in [
                "sqlite::memory:",
                "sqlite:relative.db",
                "sqlite://absolute.db?mode=rwc",
            ] {
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
