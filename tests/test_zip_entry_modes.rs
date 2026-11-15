//! IF-218 (c): `zip_create` records each source file's permission bits.
#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;
use serde_json::json;

async fn execute(node: &str, config: serde_json::Value) {
    NodeRegistry::with_builtins()
        .get(node)
        .unwrap()
        .execute(&config, &Context::new())
        .await
        .unwrap_or_else(|error| panic!("{node} failed: {error:#}"));
}

fn write(path: &Path, mode: u32) {
    std::fs::write(path, b"payload").unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
}

fn stored_modes(archive: &Path) -> Vec<(String, u32)> {
    let mut archive = zip::ZipArchive::new(std::fs::File::open(archive).unwrap()).unwrap();
    (0..archive.len())
        .map(|index| {
            let entry = archive.by_index(index).unwrap();
            // The full `st_mode` field, file-type and special bits included.
            (
                entry.name().unwrap().into_owned(),
                entry.unix_mode().unwrap() & 0o7777,
            )
        })
        .collect()
}

#[tokio::test]
async fn entries_keep_their_source_permission_bits_without_special_bits() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("source");
    std::fs::create_dir_all(source.join("bin")).unwrap();
    write(&source.join("bin/run.sh"), 0o755);
    write(&source.join("plain.txt"), 0o644);
    write(&source.join("private.key"), 0o600);
    write(&source.join("readonly.txt"), 0o444);
    // Setuid, setgid and sticky are never copied into the archive.
    write(&source.join("special"), 0o6755);
    let special = std::fs::metadata(source.join("special")).unwrap();
    assert_eq!(special.permissions().mode() & 0o7777, 0o6755);
    let archive = directory.path().join("out.zip");

    execute("zip_create", json!({"source": source, "zip_path": archive})).await;

    let mut modes = stored_modes(&archive);
    modes.sort();
    assert_eq!(
        modes
            .iter()
            .map(|(name, mode)| format!("{name}={mode:o}"))
            .collect::<Vec<_>>(),
        [
            "bin/run.sh=755",
            "plain.txt=644",
            "private.key=600",
            "readonly.txt=444",
            "special=755",
        ]
    );
}

#[tokio::test]
async fn a_single_file_source_keeps_its_mode() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("tool.sh");
    write(&source, 0o750);
    let archive = directory.path().join("out.zip");

    execute("zip_create", json!({"source": source, "zip_path": archive})).await;

    assert_eq!(stored_modes(&archive), [("tool.sh".to_owned(), 0o750)]);
}
