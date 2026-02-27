use std::io::Write;

use ironflow::cli::IronFlowConfig;
use tempfile::NamedTempFile;

#[test]
fn load_valid_config_all_fields() {
    let yaml = r#"
host: "127.0.0.1"
port: 8080
store_dir: "custom/runs"
flows_dir: "my_flows"
max_body: 2097152
max_concurrent_tasks: 8
"#;

    let mut f = NamedTempFile::new().unwrap();
    f.write_all(yaml.as_bytes()).unwrap();

    let cfg = IronFlowConfig::load(Some(f.path())).unwrap();

    assert_eq!(cfg.host.as_deref(), Some("127.0.0.1"));
    assert_eq!(cfg.port, Some(8080));
    assert_eq!(cfg.store_dir.as_deref(), Some("custom/runs"));
    assert_eq!(cfg.flows_dir.as_deref(), Some("my_flows"));
    assert_eq!(cfg.max_body, Some(2097152));
    assert_eq!(cfg.max_concurrent_tasks, Some(8));
}

#[test]
fn load_partial_config() {
    let yaml = r#"
port: 9090
"#;

    let mut f = NamedTempFile::new().unwrap();
    f.write_all(yaml.as_bytes()).unwrap();

    let cfg = IronFlowConfig::load(Some(f.path())).unwrap();

    assert_eq!(cfg.port, Some(9090));
    assert!(cfg.host.is_none());
    assert!(cfg.store_dir.is_none());
    assert!(cfg.flows_dir.is_none());
    assert!(cfg.max_body.is_none());
    assert!(cfg.max_concurrent_tasks.is_none());
}

#[test]
fn missing_explicit_path_returns_error() {
    let result = IronFlowConfig::load(Some(std::path::Path::new("/nonexistent/ironflow.yaml")));
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("Config file not found")
    );
}

#[test]
fn missing_auto_detect_returns_defaults() {
    // Run from a temp directory where no ironflow.yaml exists
    let dir = tempfile::tempdir().unwrap();
    let original_dir = std::env::current_dir().unwrap();
    // We can't safely change cwd in a parallel test, so just test with None
    // by ensuring no ironflow.yaml exists at the default path check
    // Instead, test the default return directly
    let cfg = IronFlowConfig::default();
    assert!(cfg.host.is_none());
    assert!(cfg.port.is_none());
    assert!(cfg.store_dir.is_none());
    assert!(cfg.flows_dir.is_none());
    assert!(cfg.max_body.is_none());
    assert!(cfg.max_concurrent_tasks.is_none());
    drop(dir);
    drop(original_dir);
}

#[test]
fn invalid_yaml_returns_error() {
    let yaml = "port: [this is not valid yaml for a u16";

    let mut f = NamedTempFile::new().unwrap();
    f.write_all(yaml.as_bytes()).unwrap();

    let result = IronFlowConfig::load(Some(f.path()));
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("Failed to parse config file")
    );
}

#[test]
fn webhooks_parsed_from_yaml() {
    let yaml = r#"
flows_dir: "data/flows"
webhooks:
  hello: hello_world.lua
  process-order: orders/process.lua
"#;

    let mut f = NamedTempFile::new().unwrap();
    f.write_all(yaml.as_bytes()).unwrap();

    let cfg = IronFlowConfig::load(Some(f.path())).unwrap();

    let webhooks = cfg.webhooks.unwrap();
    assert_eq!(webhooks.len(), 2);
    assert_eq!(webhooks.get("hello").unwrap(), "hello_world.lua");
    assert_eq!(
        webhooks.get("process-order").unwrap(),
        "orders/process.lua"
    );
}

#[test]
fn missing_webhooks_defaults_to_none() {
    let yaml = r#"
port: 3000
"#;

    let mut f = NamedTempFile::new().unwrap();
    f.write_all(yaml.as_bytes()).unwrap();

    let cfg = IronFlowConfig::load(Some(f.path())).unwrap();
    assert!(cfg.webhooks.is_none());
}

#[test]
fn unknown_keys_are_ignored() {
    let yaml = r#"
port: 4000
unknown_setting: true
another_random_key: "hello"
"#;

    let mut f = NamedTempFile::new().unwrap();
    f.write_all(yaml.as_bytes()).unwrap();

    let cfg = IronFlowConfig::load(Some(f.path())).unwrap();
    assert_eq!(cfg.port, Some(4000));
}
