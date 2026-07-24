//! Direct contract coverage for completed, unsuccessful shell processes.

use std::collections::HashMap;

use ironflow::engine::types::Context;
use ironflow::nodes::{NodeFailure, NodeRegistry};
use serde_json::json;

fn empty_ctx() -> Context {
    HashMap::new()
}

#[tokio::test]
async fn default_nonzero_exit_carries_structured_output() {
    let registry = NodeRegistry::with_builtins();
    let node = registry.get("shell_command").unwrap();
    let config = json!({
        "cmd": "sh",
        "args": ["-c", "printf stdout-value; printf stderr-value >&2; exit 7"],
        "output_key": "command"
    });

    let error = node.execute(&config, &empty_ctx()).await.unwrap_err();
    let failure = error.downcast_ref::<NodeFailure>().unwrap();

    assert_eq!(error.to_string(), "Command 'sh' exited with code 7");
    assert_eq!(failure.output()["command_stdout"], "stdout-value");
    assert_eq!(failure.output()["command_stderr"], "stderr-value");
    assert_eq!(failure.output()["command_code"], 7);
    assert_eq!(failure.output()["command_success"], false);
    assert!(!failure.output().contains_key("command_output_truncated"));
}

#[tokio::test]
async fn disabled_nonzero_policy_returns_unsuccessful_output() {
    let registry = NodeRegistry::with_builtins();
    let node = registry.get("shell_command").unwrap();
    let ctx = Context::from([("strict".to_string(), json!(false))]);
    let config = json!({
        "cmd": "sh",
        "args": ["-c", "printf inspectable >&2; exit 9"],
        "fail_on_nonzero": "${ctx.strict}"
    });

    let output = node.execute(&config, &ctx).await.unwrap();

    assert_eq!(output["shell_stdout"], "");
    assert_eq!(output["shell_stderr"], "inspectable");
    assert_eq!(output["shell_code"], 9);
    assert_eq!(output["shell_success"], false);
}

#[tokio::test]
async fn invalid_nonzero_policy_is_rejected() {
    let registry = NodeRegistry::with_builtins();
    let node = registry.get("shell_command").unwrap();
    let config = json!({"cmd": "true", "fail_on_nonzero": "sometimes"});

    let error = node.execute(&config, &empty_ctx()).await.unwrap_err();

    assert!(
        error
            .to_string()
            .contains("'fail_on_nonzero' must be a boolean")
    );
    assert!(error.downcast_ref::<NodeFailure>().is_none());
}

#[tokio::test]
async fn disabled_nonzero_policy_does_not_hide_operational_failures() {
    let registry = NodeRegistry::with_builtins();
    let node = registry.get("shell_command").unwrap();
    let config = json!({
        "cmd": "/ironflow/if022/command/does/not/exist",
        "fail_on_nonzero": false
    });

    let error = node.execute(&config, &empty_ctx()).await.unwrap_err();

    assert!(error.downcast_ref::<NodeFailure>().is_none());
}
