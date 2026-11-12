use super::*;
use serde_json::json;

#[test]
fn a_dynamic_entry_holds_no_item_and_reads_it_when_the_child_starts() {
    let parent = Context::from([
        ("jobs".into(), json!(["x".repeat(3 * 1024 * 1024)])),
        ("reference".into(), json!({"mapped": true})),
    ]);
    let config = json!({"flow": "child.lua", "source_key": "jobs", "input": {
        "item": "reference", "index": "reference", "mapped": "reference"
    }});
    let entry = resolve_flow_entries(&config, &parent)
        .unwrap()
        .pop()
        .unwrap();
    assert!(entry.config["input"].get("item").is_none());
    assert!(entry.config["input"].get("index").is_none());
    // The entry is a reference into the parent, not a copy of the payload.
    assert!(serde_json::to_string(entry.config.as_ref()).unwrap().len() < 256);

    let child = entry.child_context(&parent);
    assert_eq!(child["item"], parent["jobs"][0]);
    assert_eq!(child["index"], 1);
    assert_eq!(child["mapped"], parent["reference"]);
}

#[test]
fn dynamic_entries_share_one_configuration() {
    let parent = Context::from([("jobs".into(), json!([1, 2, 3]))]);
    let config = json!({"flow": "child.lua", "source_key": "jobs"});

    let entries = resolve_flow_entries(&config, &parent).unwrap();

    assert_eq!(entries.len(), 3);
    assert!(Arc::ptr_eq(&entries[0].config, &entries[2].config));
    assert_eq!(entries[2].child_context(&parent)["item"], 3);
    assert_eq!(entries[2].child_context(&parent)["index"], 3);
}

#[test]
fn user_config_cannot_populate_the_internal_literal_input_field() {
    let parent = Context::from([("customer".into(), json!("mapped-value"))]);
    let config = json!({"flows": [{
        "flow": "child.lua", "input": {"item": "customer"},
        "literal_input": {"item": "not-an-input-option"}
    }]});
    let entry = resolve_flow_entries(&config, &parent)
        .unwrap()
        .pop()
        .unwrap();
    let child = entry.child_context(&parent);
    assert_eq!(child["item"], "mapped-value");
}

#[test]
fn a_fan_out_above_the_ceiling_is_refused_in_both_modes() {
    let limit = crate::util::limits::max_parallel_subworkflows();
    let expected = format!(
        "parallel_subworkflows: {} child flows exceeds the limit of {limit} \
         (IRONFLOW_MAX_PARALLEL_SUBWORKFLOWS)",
        limit + 1
    );

    let items: Vec<Value> = (0..=limit).map(Value::from).collect();
    let parent = Context::from([("jobs".into(), Value::Array(items))]);
    let dynamic = json!({"flow": "child.lua", "source_key": "jobs"});
    let error = resolve_flow_entries(&dynamic, &parent).err().unwrap();
    assert_eq!(error.to_string(), expected);

    let flows: Vec<Value> = (0..=limit).map(|_| json!({"flow": "child.lua"})).collect();
    let error = resolve_flow_entries(&json!({ "flows": flows }), &Context::new())
        .err()
        .unwrap();
    assert_eq!(error.to_string(), expected);

    let at_limit: Vec<Value> = (0..limit).map(Value::from).collect();
    let parent = Context::from([("jobs".into(), Value::Array(at_limit))]);
    assert_eq!(
        resolve_flow_entries(&dynamic, &parent).unwrap().len(),
        limit
    );
}
