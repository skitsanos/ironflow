//! Classification of `sqlite:` connection URLs.
//!
//! The SQL stores hold a connection pool. A SQLite URL that names no database
//! file gives every pooled connection its own empty database, so the schema
//! created on one connection is missing on the next (IF-199).

use percent_encoding::percent_decode_str;

/// True when `url` selects an in-memory or temporary SQLite database instead
/// of a file. Mirrors how the pinned sqlx SQLite driver reads the URL: scheme
/// removed, split at the first `?`, database part percent-decoded.
pub(crate) fn is_ephemeral(url: &str) -> bool {
    let rest = url
        .trim_start_matches("sqlite://")
        .trim_start_matches("sqlite:");
    let (database, params) = match rest.split_once('?') {
        Some((database, params)) => (database, Some(params)),
        None => (rest, None),
    };
    let memory_mode = params.is_some_and(|params| {
        url::form_urlencoded::parse(params.as_bytes())
            .any(|(key, value)| key == "mode" && value == "memory")
    });
    if memory_mode {
        return true;
    }
    let Ok(database) = percent_decode_str(database).decode_utf8() else {
        return false;
    };
    // An empty name is a private temporary database; SQLite itself reads
    // `:memory:` and the `file:` URI forms below as in-memory or temporary.
    database.is_empty()
        || database == ":memory:"
        || database == "file:"
        || database.starts_with("file::memory:")
}

/// Diagnostic shared by the stores and the CLI. It never contains the URL.
pub(crate) const EPHEMERAL_REJECTION: &str = "in-memory and temporary SQLite databases are not \
     supported: every pooled connection would open its own empty database";

#[cfg(test)]
mod tests {
    use super::is_ephemeral;

    #[test]
    fn urls_without_a_database_file_are_ephemeral() {
        for url in [
            "sqlite::memory:",
            "sqlite://:memory:",
            "sqlite:%3Amemory%3A",
            "sqlite://?mode=memory",
            "sqlite:named.db?cache=shared&mode=memory",
            "sqlite:file::memory:",
            "sqlite:file:",
            "sqlite:",
            "sqlite://",
            "sqlite:?mode=rwc",
        ] {
            assert!(is_ephemeral(url), "{url}");
        }
    }

    #[test]
    fn urls_that_name_a_file_are_not_ephemeral() {
        for url in [
            "sqlite:relative.db",
            "sqlite://data/runs/ironflow.sqlite?mode=rwc",
            "sqlite:///var/lib/ironflow/state.db",
            "sqlite:memory.db?mode=rw",
            "sqlite:%2Ftmp%2Fmy%20runs%2Fironflow.sqlite?mode=rwc",
            "sqlite::memory:.db",
        ] {
            assert!(!is_ephemeral(url), "{url}");
        }
    }
}
