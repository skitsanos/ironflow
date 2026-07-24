use std::collections::HashSet;
use std::io;
use std::path::{Path, PathBuf};

use anyhow::Result;

/// Load one dotenv file without partially mutating the process environment.
///
/// An explicit path must identify a readable, valid file. Without an explicit
/// path, only `.env` in the current working directory is considered; an absent
/// file is the sole case that returns `Ok(None)`.
///
/// # Safety
///
/// No other thread may be running or accessing the process environment while
/// the parsed values are installed.
pub(super) unsafe fn load_dotenv(explicit_path: Option<&Path>) -> Result<Option<PathBuf>> {
    let Some(path) = dotenv_path(explicit_path)? else {
        return Ok(None);
    };

    let iterator = dotenvy::from_path_iter(&path)
        .map_err(|_| anyhow::anyhow!("Failed to read dotenv file: {}", path.display()))?;
    let entries = collect_entries(iterator, &path)?;

    for (key, value) in entries {
        if std::env::var_os(&key).is_some() {
            continue;
        }

        // SAFETY: the CLI calls this loader from its synchronous bootstrap,
        // before constructing the Tokio runtime or starting any other thread.
        // No concurrent environment read or write can therefore race with it.
        unsafe { std::env::set_var(key, value) };
    }

    Ok(Some(path))
}

fn dotenv_path(explicit_path: Option<&Path>) -> Result<Option<PathBuf>> {
    let (path, required) = match explicit_path {
        Some(path) => (path.to_path_buf(), true),
        None => {
            let cwd = std::env::current_dir()
                .map_err(|_| anyhow::anyhow!("Failed to determine the current directory"))?;
            (cwd.join(".env"), false)
        }
    };

    let link_metadata = match std::fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if !required && error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(_) if required => {
            return Err(anyhow::anyhow!(
                "Dotenv file is not accessible: {}",
                path.display()
            ));
        }
        Err(_) => {
            return Err(anyhow::anyhow!(
                "Failed to inspect dotenv file: {}",
                path.display()
            ));
        }
    };

    let metadata = if link_metadata.file_type().is_symlink() {
        std::fs::metadata(&path)
            .map_err(|_| anyhow::anyhow!("Dotenv file is not accessible: {}", path.display()))?
    } else {
        link_metadata
    };

    if !metadata.is_file() {
        return Err(anyhow::anyhow!(
            "Dotenv path is not a file: {}",
            path.display()
        ));
    }

    Ok(Some(path))
}

fn collect_entries(
    iterator: impl Iterator<Item = dotenvy::Result<(String, String)>>,
    path: &Path,
) -> Result<Vec<(String, String)>> {
    let mut seen = HashSet::new();
    let mut entries = Vec::new();

    for entry in iterator {
        let (key, value) = entry.map_err(|error| sanitized_entry_error(error, path))?;
        validate_entry(&key, &value, path)?;
        if seen.insert(key.clone()) {
            entries.push((key, value));
        }
    }

    Ok(entries)
}

fn sanitized_entry_error(error: dotenvy::Error, path: &Path) -> anyhow::Error {
    let action = if matches!(error, dotenvy::Error::Io(_)) {
        "read"
    } else {
        "parse"
    };
    anyhow::anyhow!("Failed to {action} dotenv file: {}", path.display())
}

fn validate_entry(key: &str, value: &str, path: &Path) -> Result<()> {
    let valid_key = !key.is_empty() && !key.contains(['=', '\0']);
    if !valid_key || value.contains('\0') {
        return Err(anyhow::anyhow!(
            "Dotenv file contains an invalid environment entry: {}",
            path.display()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collection_keeps_the_first_duplicate_without_touching_the_environment() {
        let entries = vec![
            Ok(("IRONFLOW_TEST_DUPLICATE".to_string(), "first".to_string())),
            Ok(("IRONFLOW_TEST_OTHER".to_string(), "value".to_string())),
            Ok(("IRONFLOW_TEST_DUPLICATE".to_string(), "second".to_string())),
        ];

        assert_eq!(
            collect_entries(entries.into_iter(), Path::new("test.env")).unwrap(),
            vec![
                ("IRONFLOW_TEST_DUPLICATE".to_string(), "first".to_string()),
                ("IRONFLOW_TEST_OTHER".to_string(), "value".to_string()),
            ]
        );
    }

    #[test]
    fn parse_errors_do_not_disclose_the_line_or_value() {
        let secret_line = "SECRET_TOKEN=do-not-disclose";
        let entries = vec![Err(dotenvy::Error::LineParse(secret_line.to_string(), 8))];

        let message = collect_entries(entries.into_iter(), Path::new("test.env"))
            .unwrap_err()
            .to_string();
        assert_eq!(message, "Failed to parse dotenv file: test.env");
        assert!(!message.contains(secret_line));
        assert!(!message.contains("do-not-disclose"));
    }

    #[test]
    fn invalid_environment_entries_do_not_disclose_contents() {
        let message = validate_entry("SECRET_TOKEN", "secret\0value", Path::new("test.env"))
            .unwrap_err()
            .to_string();

        assert_eq!(
            message,
            "Dotenv file contains an invalid environment entry: test.env"
        );
        assert!(!message.contains("SECRET_TOKEN"));
        assert!(!message.contains("secret"));
    }
}
