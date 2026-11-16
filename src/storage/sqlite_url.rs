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

/// True when `url` has the `sqlite://<relative path>` form. The connection
/// layer parses what follows `//` as a URL authority, so the first path
/// component is read as a host, and as a port or a user when it contains `:`
/// or `@`; a name with any other character fails with an unrelated error
/// (IF-199, IF-220). `sqlite:<path>` and `sqlite:///<absolute path>` are not
/// affected.
pub(crate) fn has_relative_authority(url: &str) -> bool {
    url.strip_prefix("sqlite://").is_some_and(|rest| {
        let authority = rest.split(['/', '?']).next().unwrap_or_default();
        !authority.is_empty()
    })
}

/// Diagnostic for [`has_relative_authority`]. It never contains the URL.
pub(crate) const RELATIVE_AUTHORITY_REJECTION: &str = "a sqlite://<relative path> URL is not \
     supported, because its first path component is read as a host name: write a relative path \
     as sqlite:<path> and an absolute one as sqlite:///<path>";

/// Diagnostic shared by the stores and the CLI. It never contains the URL.
pub(crate) const EPHEMERAL_REJECTION: &str = "in-memory and temporary SQLite databases are not \
     supported: every pooled connection would open its own empty database";

#[cfg(test)]
mod tests {
    use super::{has_relative_authority, is_ephemeral};

    /// The wording is pinned whole: indentation of the source once leaked
    /// into it as runs of spaces.
    #[test]
    fn the_relative_authority_rejection_reads_as_one_sentence() {
        assert_eq!(
            super::RELATIVE_AUTHORITY_REJECTION,
            "a sqlite://<relative path> URL is not supported, because its first path component \
             is read as a host name: write a relative path as sqlite:<path> and an absolute one \
             as sqlite:///<path>"
        );
        for message in [
            super::RELATIVE_AUTHORITY_REJECTION,
            super::EPHEMERAL_REJECTION,
        ] {
            assert!(!message.contains("  "), "{message}");
        }
    }

    #[test]
    fn only_the_double_slash_form_with_a_relative_path_has_an_authority() {
        for url in [
            "sqlite://data/runs/ironflow.sqlite?mode=rwc",
            "sqlite://state.db",
            "sqlite://user@dir/state.db",
            "sqlite://dir:1/state.db",
        ] {
            assert!(has_relative_authority(url), "{url}");
        }
        for url in [
            "sqlite:relative.db",
            "sqlite:data/runs/ironflow.sqlite?mode=rwc",
            "sqlite:///var/lib/ironflow/state.db",
            "sqlite:%2Ftmp%2Fstate.db?mode=rwc",
            "sqlite://",
            "sqlite://?mode=memory",
            "postgres://host/db",
        ] {
            assert!(!has_relative_authority(url), "{url}");
        }
    }

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
