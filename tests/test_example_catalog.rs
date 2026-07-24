use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use ironflow::lua::LuaRuntime;
use ironflow::nodes::NodeRegistry;
use serde::Deserialize;

const REQUIRED_LABELS: [&str; 4] = [
    "credentialed",
    "external_service",
    "local_state",
    "platform_specific",
];

#[derive(Debug, Deserialize)]
struct ExampleCatalog {
    schema_version: u64,
    categories: BTreeMap<String, FlowGroup>,
    labels: BTreeMap<String, FlowGroup>,
    capabilities: BTreeMap<String, FlowGroup>,
    node_coverage: NodeCoverage,
}

#[derive(Debug, Deserialize)]
struct FlowGroup {
    description: String,
    flows: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct NodeCoverage {
    exemptions: BTreeMap<String, String>,
}

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn catalog() -> ExampleCatalog {
    let path = repository_root().join("examples/catalog.json");
    let bytes = fs::read(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
    serde_json::from_slice(&bytes)
        .unwrap_or_else(|error| panic!("failed to parse {}: {error}", path.display()))
}

fn collect_lua_files(dir: &Path, root: &Path, output: &mut BTreeSet<String>) {
    let entries = fs::read_dir(dir)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", dir.display()));
    for entry in entries {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect_lua_files(&path, root, output);
        } else if path.extension().and_then(|value| value.to_str()) == Some("lua") {
            output.insert(
                path.strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
    }
}

fn lua_files() -> BTreeSet<String> {
    let root = repository_root();
    let mut files = BTreeSet::new();
    collect_lua_files(&root.join("examples"), &root, &mut files);
    files
}

fn checked_group(
    kind: &str,
    name: &str,
    group: &FlowGroup,
    actual: &BTreeSet<String>,
) -> BTreeSet<String> {
    assert!(
        !group.description.trim().is_empty(),
        "{kind} '{name}' needs a non-empty description"
    );
    assert!(!group.flows.is_empty(), "{kind} '{name}' has no flows");

    let flows = group.flows.iter().cloned().collect::<BTreeSet<_>>();
    assert_eq!(
        flows.len(),
        group.flows.len(),
        "{kind} '{name}' contains duplicate flow references"
    );
    for flow in &flows {
        assert!(
            actual.contains(flow),
            "{kind} '{name}' references missing flow {flow}"
        );
    }
    flows
}

#[test]
fn catalog_classifies_and_labels_every_lua_flow_consistently() {
    let catalog = catalog();
    let actual = lua_files();
    assert_eq!(catalog.schema_version, 2);

    let mut category_occurrences = BTreeMap::<String, usize>::new();
    let mut category_flows = BTreeMap::<String, BTreeSet<String>>::new();
    for (name, group) in &catalog.categories {
        let flows = checked_group("category", name, group, &actual);
        for flow in &flows {
            *category_occurrences.entry(flow.clone()).or_default() += 1;
        }
        category_flows.insert(name.clone(), flows);
    }

    let classified = category_occurrences
        .keys()
        .cloned()
        .collect::<BTreeSet<_>>();
    assert_eq!(
        classified, actual,
        "catalog categories and filesystem differ"
    );
    for (flow, count) in category_occurrences {
        assert_eq!(count, 1, "{flow} is classified {count} times");
    }

    let expected_labels = REQUIRED_LABELS
        .into_iter()
        .map(str::to_string)
        .collect::<BTreeSet<_>>();
    let actual_labels = catalog.labels.keys().cloned().collect::<BTreeSet<_>>();
    assert_eq!(actual_labels, expected_labels, "catalog labels differ");

    let label_flows = catalog
        .labels
        .iter()
        .map(|(name, group)| (name.as_str(), checked_group("label", name, group, &actual)))
        .collect::<BTreeMap<_, _>>();

    let public_network = category_flows
        .get("public_network")
        .expect("missing public_network category");
    let credentialed_external = category_flows
        .get("credentialed_external")
        .expect("missing credentialed_external category");
    let expected_external = public_network
        .union(credentialed_external)
        .cloned()
        .collect::<BTreeSet<_>>();
    assert_eq!(
        label_flows["external_service"], expected_external,
        "external_service must equal public_network plus credentialed_external"
    );
    assert_eq!(
        label_flows["credentialed"], *credentialed_external,
        "credentialed must equal credentialed_external"
    );

    assert!(
        !catalog.capabilities.is_empty(),
        "catalog needs at least one platform capability"
    );
    let mut capability_union = BTreeSet::new();
    for (name, group) in &catalog.capabilities {
        assert!(!name.trim().is_empty(), "capability name cannot be blank");
        capability_union.extend(checked_group("capability", name, group, &actual));
    }
    assert_eq!(
        label_flows["platform_specific"], capability_union,
        "platform_specific must equal the union of capability flows"
    );
}

#[test]
fn evaluated_examples_validate_and_cover_the_builtin_registry() {
    let root = repository_root();
    let catalog = catalog();
    let registry = NodeRegistry::with_builtins();
    let registered = registry
        .list()
        .into_iter()
        .map(|(node_type, _)| node_type.to_string())
        .collect::<BTreeSet<_>>();
    let mut used = BTreeSet::new();
    let mut errors = Vec::new();

    for relative_path in lua_files() {
        let path = root.join(&relative_path);
        let Some(path_text) = path.to_str() else {
            errors.push(format!("{relative_path}: path is not UTF-8"));
            continue;
        };
        let flow = match LuaRuntime::load_flow(path_text, &registry) {
            Ok(flow) => flow,
            Err(error) => {
                errors.push(format!("{relative_path}: failed to load: {error:#}"));
                continue;
            }
        };

        for error in flow.validate_dag() {
            errors.push(format!("{relative_path}: invalid graph: {error}"));
        }
        for step in flow.steps {
            if registry.get(&step.node_type).is_none() {
                errors.push(format!(
                    "{relative_path}: step '{}' uses unknown node type '{}'",
                    step.name, step.node_type
                ));
            }
            used.insert(step.node_type);
        }
    }

    for (node_type, reason) in &catalog.node_coverage.exemptions {
        if reason.trim().is_empty() {
            errors.push(format!(
                "node coverage exemption '{node_type}' needs a non-empty reason"
            ));
        }
        if !registered.contains(node_type) {
            errors.push(format!(
                "node coverage exemption '{node_type}' is not registered"
            ));
        }
        if used.contains(node_type) {
            errors.push(format!(
                "node coverage exemption '{node_type}' is stale because an example uses it"
            ));
        }
    }

    for node_type in &registered {
        if !used.contains(node_type) && !catalog.node_coverage.exemptions.contains_key(node_type) {
            errors.push(format!(
                "registered node '{node_type}' has no example or documented exemption"
            ));
        }
    }

    assert!(
        errors.is_empty(),
        "example validation failed:\n{}",
        errors.join("\n")
    );
}

#[test]
fn examples_avoid_ignored_inputs_and_shared_machine_paths() {
    let root = repository_root();
    let forbidden = [
        ("data/samples", "ignored sample-data path"),
        ("\"/tmp/", "hard-coded absolute temporary path"),
        ("sqlite:/tmp", "shared SQLite temporary path"),
    ];

    let mut errors = Vec::new();
    for flow in lua_files() {
        let source = fs::read_to_string(root.join(&flow)).unwrap();
        for (needle, description) in forbidden {
            if source.contains(needle) {
                errors.push(format!("{flow}: contains {description} ({needle})"));
            }
        }
    }

    assert!(
        errors.is_empty(),
        "examples retain non-portable paths:\n{}",
        errors.join("\n")
    );
}
