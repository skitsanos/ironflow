//! Tests for engine types: FlowDefinition, validate_dag, status enums, etc.

use ironflow::engine::types::*;

// --- RetryConfig ---

#[test]
fn retry_config_default() {
    let rc = RetryConfig::default();
    assert_eq!(rc.max_retries, 0);
    assert!((rc.backoff_s - 1.0).abs() < f64::EPSILON);
}

// --- TaskState ---

#[test]
fn task_state_new() {
    let ts = TaskState::new("step1", "log");
    assert_eq!(ts.name, "step1");
    assert_eq!(ts.node_type, "log");
    assert_eq!(ts.status, TaskStatus::Pending);
    assert_eq!(ts.attempt, 0);
    assert!(ts.input.is_none());
    assert!(ts.output.is_none());
    assert!(ts.error.is_none());
    assert!(ts.started.is_none());
    assert!(ts.finished.is_none());
}

// --- RunStatus / TaskStatus Display ---

#[test]
fn run_status_display() {
    assert_eq!(RunStatus::Pending.to_string(), "pending");
    assert_eq!(RunStatus::Running.to_string(), "running");
    assert_eq!(RunStatus::Success.to_string(), "success");
    assert_eq!(RunStatus::Failed.to_string(), "failed");
    assert_eq!(RunStatus::Stalled.to_string(), "stalled");
}

#[test]
fn task_status_display() {
    assert_eq!(TaskStatus::Pending.to_string(), "pending");
    assert_eq!(TaskStatus::Running.to_string(), "running");
    assert_eq!(TaskStatus::Success.to_string(), "success");
    assert_eq!(TaskStatus::Failed.to_string(), "failed");
    assert_eq!(TaskStatus::Skipped.to_string(), "skipped");
}

// --- RunStatus serialization ---

#[test]
fn run_status_serializes_lowercase() {
    let json = serde_json::to_string(&RunStatus::Success).unwrap();
    assert_eq!(json, r#""success""#);
    let back: RunStatus = serde_json::from_str(&json).unwrap();
    assert_eq!(back, RunStatus::Success);
}

// --- FlowDefinition::validate_dag ---

fn make_step(name: &str, deps: Vec<&str>) -> StepDefinition {
    StepDefinition {
        name: name.to_string(),
        node_type: "log".to_string(),
        config: serde_json::json!({}),
        dependencies: deps.into_iter().map(String::from).collect(),
        retry: RetryConfig::default(),
        timeout_s: None,
        route: None,
        on_error: None,
    }
}

#[test]
fn validate_dag_empty_flow() {
    let flow = FlowDefinition {
        name: "empty".to_string(),
        steps: vec![],
    };
    assert!(flow.validate_dag().is_empty());
}

#[test]
fn validate_dag_no_deps() {
    let flow = FlowDefinition {
        name: "parallel".to_string(),
        steps: vec![make_step("a", vec![]), make_step("b", vec![])],
    };
    assert!(flow.validate_dag().is_empty());
}

#[test]
fn validate_dag_linear_chain() {
    let flow = FlowDefinition {
        name: "chain".to_string(),
        steps: vec![
            make_step("a", vec![]),
            make_step("b", vec!["a"]),
            make_step("c", vec!["b"]),
        ],
    };
    assert!(flow.validate_dag().is_empty());
}

#[test]
fn validate_dag_missing_dependency() {
    let flow = FlowDefinition {
        name: "broken".to_string(),
        steps: vec![make_step("a", vec!["nonexistent"])],
    };
    let errors = flow.validate_dag();
    assert_eq!(errors.len(), 1);
    assert!(errors[0].contains("nonexistent"));
    assert!(errors[0].contains("does not exist"));
}

#[test]
fn validate_dag_simple_cycle() {
    let flow = FlowDefinition {
        name: "cycle".to_string(),
        steps: vec![make_step("a", vec!["b"]), make_step("b", vec!["a"])],
    };
    let errors = flow.validate_dag();
    assert_eq!(errors.len(), 1);
    assert!(errors[0].contains("Cycle"));
}

#[test]
fn validate_dag_three_step_cycle() {
    let flow = FlowDefinition {
        name: "cycle3".to_string(),
        steps: vec![
            make_step("a", vec!["c"]),
            make_step("b", vec!["a"]),
            make_step("c", vec!["b"]),
        ],
    };
    let errors = flow.validate_dag();
    assert!(!errors.is_empty());
    assert!(errors[0].contains("Cycle"));
}

#[test]
fn validate_dag_diamond() {
    // a -> b, a -> c, b -> d, c -> d
    let flow = FlowDefinition {
        name: "diamond".to_string(),
        steps: vec![
            make_step("a", vec![]),
            make_step("b", vec!["a"]),
            make_step("c", vec!["a"]),
            make_step("d", vec!["b", "c"]),
        ],
    };
    assert!(flow.validate_dag().is_empty());
}
