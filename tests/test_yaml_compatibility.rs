use std::collections::HashMap;
use std::io::Write;

use ironflow::cli::IronFlowConfig;
use ironflow::nodes::NodeRegistry;
use serde_json::{Value, json};
use tempfile::NamedTempFile;

fn load_config(yaml: &str) -> anyhow::Result<IronFlowConfig> {
    let mut file = NamedTempFile::new()?;
    file.write_all(yaml.as_bytes())?;
    IronFlowConfig::load(Some(file.path()))
}

async fn parse_yaml(yaml: &str, source_key: bool) -> anyhow::Result<Value> {
    let registry = NodeRegistry::with_builtins();
    let node = registry.get("yaml_parse").unwrap();
    let (config, context) = if source_key {
        (
            json!({"source_key": "source"}),
            HashMap::from([("source".to_string(), json!(yaml))]),
        )
    } else {
        (json!({"input": yaml}), HashMap::new())
    };
    Ok(node.execute(&config, &context).await?["yaml_data"].clone())
}

#[test]
fn config_expands_merge_keys_and_keeps_explicit_overrides() {
    let config = load_config(
        "<<: &defaults {host: localhost, port: 8080, allow_adhoc_flows: false}\nport: 9090\n",
    )
    .unwrap();
    assert_eq!(config.allow_adhoc_flows, Some(false));
    assert_eq!(config.host.as_deref(), Some("localhost"));
    assert_eq!(config.port, Some(9090));
}

#[test]
fn config_leading_zero_numbers_remain_decimal() {
    let config = load_config("port: 0123\nmax_concurrent_tasks: 0008\n").unwrap();
    assert_eq!(config.port, Some(123));
    assert_eq!(config.max_concurrent_tasks, Some(8));
}

#[tokio::test]
async fn node_expands_merge_keys_on_both_input_paths() {
    let yaml = "defaults: &defaults {host: localhost, port: 8080}\nserver:\n  <<: *defaults\n  port: 9090\n";
    for source_key in [false, true] {
        let value = parse_yaml(yaml, source_key).await.unwrap();
        assert_eq!(value["server"], json!({"host": "localhost", "port": 9090}));
        assert!(value["server"].get("<<").is_none());
    }
}

#[tokio::test]
async fn node_preserves_decimal_and_quoted_leading_zero_values() {
    for source_key in [false, true] {
        let value = parse_yaml("number: 0123\nquoted: '0123'\nzero: 0008\n", source_key)
            .await
            .unwrap();
        assert_eq!(value, json!({"number": 123, "quoted": "0123", "zero": 8}));
    }
}

#[tokio::test]
async fn node_preserves_yaml12_scalars_and_scalar_tags() {
    let value = parse_yaml(
        "country: NO\nflag: true\nempty: null\nbinary: 0b11\ntagged: !label example\n",
        false,
    )
    .await
    .unwrap();
    assert_eq!(
        value,
        json!({"country": "NO", "flag": true, "empty": null, "binary": "0b11", "tagged": "example"})
    );
}

#[tokio::test]
async fn stringify_parse_preserves_json_scalar_types() {
    let registry = NodeRegistry::with_builtins();
    let stringify = registry.get("yaml_stringify").unwrap();
    let value =
        json!({"quoted": "0123", "number": 123, "binary": "0b11", "flag": true, "empty": null});
    let context = HashMap::from([("data".to_string(), value.clone())]);
    let output = stringify
        .execute(&json!({"source_key": "data"}), &context)
        .await
        .unwrap();
    assert_eq!(
        parse_yaml(output["yaml"].as_str().unwrap(), true)
            .await
            .unwrap(),
        value
    );
}

#[tokio::test]
async fn node_rejects_recursive_aliases_and_excessive_expansion() {
    let repeated = format!("anchor: &a [value]\nexpanded: [{}]\n", "*a,".repeat(1100));
    for yaml in ["value: &a [*a]\n", repeated.as_str()] {
        for source_key in [false, true] {
            assert!(parse_yaml(yaml, source_key).await.is_err());
        }
    }
}

#[tokio::test]
async fn node_rejects_excessive_nesting_on_both_input_paths() {
    let yaml = format!("{}0{}", "[".repeat(200), "]".repeat(200));
    for source_key in [false, true] {
        assert!(parse_yaml(&yaml, source_key).await.is_err());
    }
}
