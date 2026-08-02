use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::path::{Path, PathBuf};

use anyhow::Result;

use super::super::helpers::{ZipLimits, validate_zip_entry_name};
use super::copy::copy_with_control;
use super::read::{add_uncompressed_size, open_zip_file, validate_entry_count};
use super::rooted::RootedDir;
use crate::util::execution::ExecutionControl;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EntryKind {
    Directory,
    File,
}

struct EntryPlan {
    name: String,
    relative: PathBuf,
    kind: EntryKind,
}

pub(super) fn extract_zip_archive(
    zip_path: &str,
    destination: &str,
    overwrite: bool,
    limits: ZipLimits,
    execution: &ExecutionControl,
) -> Result<Vec<String>> {
    execution.checkpoint()?;
    let file = open_zip_file(zip_path, "zip_extract")?;
    let mut archive = zip::ZipArchive::new(file).map_err(|error| {
        anyhow::anyhow!(
            "zip_extract: '{}' is not a valid ZIP archive: {}",
            zip_path,
            error
        )
    })?;
    let plans = preflight(&mut archive, limits, execution)?;
    execution.checkpoint()?;

    let root = RootedDir::prepare(Path::new(destination), "zip_extract", execution)?;
    let mut extracted = Vec::with_capacity(plans.len());
    let mut actual_total = 0u64;

    for (index, plan) in plans.into_iter().enumerate() {
        execution.checkpoint()?;
        match plan.kind {
            EntryKind::Directory => root.ensure_dir(&plan.relative, execution)?,
            EntryKind::File => {
                let mut entry = archive.by_index(index)?;
                let mut staged = root.stage_file(&plan.relative, overwrite, execution)?;
                let remaining = limits
                    .max_total_uncompressed_bytes
                    .saturating_sub(actual_total);
                let copied = copy_with_control(
                    &mut entry,
                    staged.writer(),
                    execution,
                    remaining,
                    "zip_extract",
                )?;
                actual_total = actual_total.saturating_add(copied);
                execution.checkpoint()?;
                staged.commit()?;
            }
        }
        extracted.push(plan.name);
    }
    execution.checkpoint()?;
    Ok(extracted)
}

fn preflight(
    archive: &mut zip::ZipArchive<File>,
    limits: ZipLimits,
    execution: &ExecutionControl,
) -> Result<Vec<EntryPlan>> {
    validate_entry_count("zip_extract", archive.len(), limits)?;
    let mut plans = Vec::with_capacity(archive.len());
    let mut declared_total = 0u64;
    let mut seen = HashMap::<String, EntryKind>::new();
    let mut required_directories = HashSet::<String>::new();

    for index in 0..archive.len() {
        execution.checkpoint()?;
        let entry = archive.by_index(index)?;
        add_uncompressed_size("zip_extract", &mut declared_total, entry.size(), limits)?;
        let kind = validate_entry_type(&entry)?;
        let name = entry.name().to_string();
        let relative =
            validate_zip_entry_name(&name, kind == EntryKind::Directory, limits.max_depth)?;
        let collision_key = collision_key(&relative);
        if kind == EntryKind::File && required_directories.contains(&collision_key) {
            anyhow::bail!("zip_extract: archive file is also used as a parent directory: {name}");
        }
        if seen.insert(collision_key.clone(), kind).is_some() {
            anyhow::bail!("zip_extract: duplicate archive destination: {name}");
        }
        require_directory_parents(&relative, &seen, &mut required_directories, &name)?;
        plans.push(EntryPlan {
            name,
            relative,
            kind,
        });
    }
    Ok(plans)
}

fn validate_entry_type(entry: &zip::read::ZipFile<'_, File>) -> Result<EntryKind> {
    if entry.is_symlink() {
        anyhow::bail!(
            "zip_extract: archive symlink entries are not supported: {}",
            entry.name()
        );
    }
    let kind = if entry.is_dir() {
        EntryKind::Directory
    } else {
        EntryKind::File
    };
    if let Some(mode) = entry.unix_mode() {
        let file_type = mode & 0o170000;
        let expected = match kind {
            EntryKind::Directory => 0o040000,
            EntryKind::File => 0o100000,
        };
        if file_type != 0 && file_type != expected {
            anyhow::bail!(
                "zip_extract: archive entry is not a regular file or directory: {}",
                entry.name()
            );
        }
    }
    Ok(kind)
}

fn collision_key(path: &Path) -> String {
    path.iter()
        .map(|component| component.to_string_lossy().to_lowercase())
        .collect::<Vec<_>>()
        .join("/")
}

fn require_directory_parents(
    path: &Path,
    seen: &HashMap<String, EntryKind>,
    required_directories: &mut HashSet<String>,
    name: &str,
) -> Result<()> {
    let mut parent = path.parent();
    while let Some(path) = parent.filter(|path| !path.as_os_str().is_empty()) {
        let key = collision_key(path);
        if seen.get(&key) == Some(&EntryKind::File) {
            anyhow::bail!("zip_extract: archive file is also used as a parent directory: {name}");
        }
        required_directories.insert(key);
        parent = path.parent();
    }
    Ok(())
}
