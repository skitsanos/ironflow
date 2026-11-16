//! Connection URL for the default SQLite files under the store directory.
//!
//! The SQL stores share one `sqlx::Any` pool type, which accepts only a URL,
//! and the SQLite driver percent-decodes the database part of that URL. A raw
//! path formatted into a URL is therefore read as URL syntax (IF-199): a
//! relative first component becomes a host, `?` starts the query and `%41`
//! is decoded to a different directory. The path is instead made absolute and
//! every byte outside `[A-Za-z0-9._-]` is percent-encoded, separators
//! included, so the URL has an opaque path that the URL parser leaves alone
//! and the driver decodes back to exactly the path given.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result};

/// Absolute path of `filename` inside `store_dir`, without resolving links.
///
/// The path must be absolute before it reaches SQLite: the driver opens files
/// with URI filenames enabled, so a relative directory named `file:x` would
/// be read as a URI and opened somewhere else.
pub(super) fn database_file(store_dir: &Path, filename: &str) -> Result<PathBuf> {
    let directory = std::path::absolute(store_dir)
        .with_context(|| format!("Failed to resolve store dir: {}", store_dir.display()))?;
    Ok(directory.join(filename))
}

/// `sqlite:` URL that opens `file`, creating it when missing.
pub(super) fn create_url(file: &Path) -> Result<String> {
    let Some(path) = file.to_str() else {
        anyhow::bail!(
            "Store dir must be valid UTF-8 for the SQLite backend: {}",
            file.display()
        );
    };
    if path.contains('\0') {
        anyhow::bail!("Store dir must not contain a NUL character");
    }
    Ok(encoded_url(path))
}

fn encoded_url(path: &str) -> String {
    let mut url = String::with_capacity(path.len() + 24);
    url.push_str("sqlite:");
    for byte in path.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-') {
            url.push(char::from(byte));
        } else {
            // Writing to a String cannot fail.
            let _ = write!(url, "%{byte:02X}");
        }
    }
    url.push_str("?mode=rwc");
    url
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::str::FromStr as _;

    use sqlx::ConnectOptions as _;
    use sqlx::sqlite::SqliteConnectOptions;

    use super::{create_url, database_file, encoded_url};

    /// The filename the pinned driver opens for `url`, taking the same route
    /// as the `Any` pool: parse as a URL first, then as SQLite options.
    fn opened(url: &str) -> String {
        let parsed = sqlx::any::AnyConnectOptions::from_str(url).unwrap();
        let options = SqliteConnectOptions::from_url(&parsed.database_url).unwrap();
        options.get_filename().to_str().unwrap().to_string()
    }

    #[test]
    fn unix_paths_reach_the_driver_unchanged() {
        for path in [
            "/var/lib/ironflow/runs/ironflow.sqlite",
            "/srv/My Runs/ironflow.sqlite",
            "/srv/runs%41data/ironflow.sqlite",
            "/srv/a?b/ironflow.sqlite",
            "/srv/a#b/ironflow.sqlite",
            "/srv/50% & more=1;x/ironflow.sqlite",
            "/srv/work/../runs/./ironflow.sqlite",
            "/srv/work/file:x/ironflow.sqlite",
            "/srv/données/运行/ironflow.sqlite",
            "/srv/trailing space /ironflow.sqlite",
            "/srv/user@host:5432/ironflow.sqlite",
        ] {
            assert_eq!(opened(&encoded_url(path)), path);
        }
    }

    /// Windows is a release target without a test job, so the shapes its
    /// absolute paths take are checked here as strings.
    #[test]
    fn windows_shaped_paths_reach_the_driver_unchanged() {
        for path in [
            r"C:\ironflow\runs\ironflow.sqlite",
            r"C:\Program Files\IronFlow 100%\a?b#c\ironflow.sqlite",
            r"D:\runs%41data\ironflow.sqlite",
            "C:/ironflow/runs/ironflow.sqlite",
            r"C:\work\..\runs\ironflow.sqlite",
            r"\\server\share\iron flow\ironflow.sqlite",
            r"\\?\C:\very long\ironflow.sqlite",
        ] {
            assert_eq!(opened(&encoded_url(path)), path);
        }
    }

    #[test]
    fn generated_urls_create_the_file_and_carry_no_url_syntax_from_the_path() {
        let url = encoded_url("/srv/a?mode=ro&vfs=x#frag/ironflow.sqlite");
        assert_eq!(
            url,
            "sqlite:%2Fsrv%2Fa%3Fmode%3Dro%26vfs%3Dx%23frag%2Fironflow.sqlite?mode=rwc"
        );
        assert!(!crate::storage::sqlite_url::is_ephemeral(&url));
    }

    #[test]
    fn relative_store_directories_become_absolute_files() {
        let file = database_file(Path::new("file:x"), "ironflow.sqlite").unwrap();
        assert!(file.is_absolute());
        assert!(file.ends_with("file:x/ironflow.sqlite"));
        let opened = opened(&create_url(&file).unwrap());
        assert!(!opened.starts_with("file:"));
        assert_eq!(Path::new(&opened), file);
    }

    #[cfg(unix)]
    #[test]
    fn a_path_that_is_not_utf8_is_refused() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt as _;

        let file = Path::new("/srv").join(OsStr::from_bytes(b"runs-\xff"));
        let error = create_url(&file).unwrap_err().to_string();
        assert!(error.contains("valid UTF-8"), "{error}");
    }
}
