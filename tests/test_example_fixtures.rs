use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

use sha2::{Digest, Sha256};

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn catalog() -> serde_json::Value {
    let path = repository_root().join("examples/catalog.json");
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

#[test]
fn fixture_checksums_match_the_reviewed_byte_set() {
    let fixture_dir = repository_root().join("examples/fixtures");
    let checksum_file = fs::read_to_string(fixture_dir.join("SHA256SUMS")).unwrap();
    let metadata_names = BTreeSet::from(["LICENSE-CC0-1.0.txt", "README.md", "SHA256SUMS"]);
    for name in &metadata_names {
        assert!(
            fixture_dir.join(name).is_file(),
            "missing fixture metadata: {name}"
        );
    }
    let payload_names = fs::read_dir(&fixture_dir)
        .unwrap()
        .map(|entry| entry.unwrap())
        .filter(|entry| entry.file_type().unwrap().is_file())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| !metadata_names.contains(name.as_str()))
        .collect::<BTreeSet<_>>();

    let mut names = BTreeSet::new();
    let mut total_bytes = 0_u64;
    for line in checksum_file.lines() {
        let (expected_hash, name) = line
            .split_once("  ")
            .unwrap_or_else(|| panic!("invalid SHA256SUMS line: {line}"));
        assert_eq!(expected_hash.len(), 64, "invalid SHA-256 for {name}");
        assert!(
            expected_hash.bytes().all(|byte| byte.is_ascii_hexdigit()),
            "invalid SHA-256 for {name}"
        );
        assert!(
            names.insert(name.to_string()),
            "duplicate checksum entry for {name}"
        );

        let bytes = fs::read(fixture_dir.join(name)).unwrap();
        total_bytes += u64::try_from(bytes.len()).unwrap();
        let actual_hash = hex::encode(Sha256::digest(bytes));
        assert_eq!(actual_hash, expected_hash, "fixture checksum drift: {name}");
    }

    assert_eq!(names, payload_names, "SHA256SUMS must list every payload");
    assert!(
        total_bytes <= 100 * 1024,
        "fixture pack grew beyond 100 KiB: {total_bytes} bytes"
    );
}

#[test]
fn clean_checkout_runtime_matrix_runs_from_an_isolated_working_directory() {
    let root = repository_root();
    let document = catalog();
    let offline = document["categories"]["offline"]["flows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap())
        .collect::<BTreeSet<_>>();
    let offline_output = document["categories"]["offline_output"]["flows"]
        .as_array()
        .unwrap();
    let offline_output = offline_output
        .iter()
        .map(|value| value.as_str().unwrap())
        .collect::<BTreeSet<_>>();
    let mut matrix = document["runtime_matrix"]["fixture_backed_offline"]
        .as_array()
        .unwrap()
        .iter()
        .collect::<Vec<_>>();
    matrix.extend(
        document["runtime_matrix"]["isolated_local_process"]
            .as_array()
            .unwrap(),
    );
    let workspace = tempfile::tempdir().unwrap();

    for (index, flow) in matrix.into_iter().enumerate() {
        let flow = flow.as_str().unwrap();
        assert!(
            offline.contains(flow) || offline_output.contains(flow),
            "runtime case is not classified as a local flow: {flow}"
        );

        let case_root = workspace.path().join(format!("case-{index}"));
        fs::create_dir_all(&case_root).unwrap();
        let store = case_root.join("store");
        let artifacts = case_root.join("artifacts");
        let output = Command::new(env!("CARGO_BIN_EXE_ironflow"))
            .current_dir(workspace.path())
            .env_remove("IRONFLOW_STORE")
            .env_remove("IRONFLOW_STORE_URL")
            .env_remove("IRONFLOW_SQL_TABLE_PREFIX")
            .env_remove("REDIS_URL")
            .env("TMPDIR", &case_root)
            .env("TMP", &case_root)
            .env("TEMP", &case_root)
            .env("IRONFLOW_ARTIFACT_DIR", artifacts)
            .arg("run")
            .arg(root.join(flow))
            .arg("--store-dir")
            .arg(store)
            .output()
            .unwrap();

        assert!(
            output.status.success(),
            "{flow} failed from isolated cwd\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("Status: success"),
            "{flow} did not report a successful terminal run"
        );
    }
}
