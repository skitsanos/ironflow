use std::fs;
use std::path::Path;

#[test]
fn examples_readme_includes_foreach_example_entry() {
    let examples_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples");
    let transforms_dir = examples_dir.join("02-data-transforms");
    let readme_path = examples_dir.join("README.md");

    let foreach_file = fs::read_dir(&transforms_dir)
        .unwrap()
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .find(|name| name.contains("foreach"))
        .expect("foreach example missing in examples/02-data-transforms");

    let readme = fs::read_to_string(readme_path).unwrap();
    assert!(
        readme.contains(&foreach_file),
        "examples/README.md should reference {}",
        foreach_file
    );
}
