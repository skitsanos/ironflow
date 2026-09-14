#[cfg(not(unix))]
mod portable;
#[cfg(unix)]
mod unix;

#[cfg(not(unix))]
pub(crate) use portable::RootedDir;
#[cfg(unix)]
pub(crate) use unix::RootedDir;

// The caller chooses the root, including any directory alias. Archive entries
// are resolved below this anchor using the platform's no-follow safeguards.
fn destination_anchor(
    path: &std::path::Path,
    operation: &str,
    execution: &crate::util::execution::ExecutionControl,
) -> anyhow::Result<(std::path::PathBuf, Vec<std::ffi::OsString>)> {
    use anyhow::Context;
    use std::path::Path;
    let path = if path.as_os_str().is_empty() {
        Path::new(".")
    } else {
        path
    };
    let mut cursor = path.to_path_buf();
    let mut missing = Vec::new();
    loop {
        execution.checkpoint()?;
        match std::fs::symlink_metadata(&cursor) {
            Ok(_) => {
                let anchor = std::fs::canonicalize(&cursor).with_context(|| {
                    format!(
                        "{operation}: cannot resolve destination directory '{}'",
                        cursor.display()
                    )
                })?;
                if !std::fs::metadata(&anchor)?.is_dir() {
                    anyhow::bail!(
                        "{operation}: resolved destination '{}' is not a directory",
                        anchor.display()
                    );
                }
                return Ok((anchor, missing));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let name = cursor.file_name().ok_or_else(|| {
                    anyhow::anyhow!(
                        "{operation}: destination '{}' has no creatable component",
                        path.display()
                    )
                })?;
                missing.push(name.to_os_string());
                cursor = cursor
                    .parent()
                    .filter(|parent| !parent.as_os_str().is_empty())
                    .unwrap_or_else(|| Path::new("."))
                    .to_path_buf();
            }
            Err(error) => return Err(error.into()),
        }
    }
}
