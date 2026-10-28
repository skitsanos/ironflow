//! Integration tests for workflow engine semantics, Lua runtime, composition nodes and schemas.
//!
//! One binary per area keeps the number of linked test executables bounded
//! (IF-162). Every module is an unchanged test file under `tests/`; run one
//! with `cargo test --test engine <module>::`.

#[path = "../test_adhoc_flow_policy.rs"]
mod test_adhoc_flow_policy;
#[path = "../test_child_failure_details.rs"]
mod test_child_failure_details;
#[path = "../test_child_result_fidelity.rs"]
mod test_child_result_fidelity;
#[path = "../test_context_projection_workflow.rs"]
mod test_context_projection_workflow;
#[path = "../test_engine.rs"]
mod test_engine;
#[path = "../test_engine_terminalization.rs"]
mod test_engine_terminalization;
#[path = "../test_flow_file_disclosure.rs"]
mod test_flow_file_disclosure;
#[path = "../test_foreach_node.rs"]
mod test_foreach_node;
#[path = "../test_interpolation_validation.rs"]
mod test_interpolation_validation;
#[path = "../test_lua_context_projection.rs"]
mod test_lua_context_projection;
#[path = "../test_lua_runtime.rs"]
mod test_lua_runtime;
#[path = "../test_nested_workflow_cancellation.rs"]
mod test_nested_workflow_cancellation;
#[path = "../test_node_config.rs"]
mod test_node_config;
#[path = "../test_node_config_interpolation.rs"]
mod test_node_config_interpolation;
#[path = "../test_on_error_recovery.rs"]
mod test_on_error_recovery;
#[path = "../test_parallel_context_determinism.rs"]
mod test_parallel_context_determinism;
#[path = "../test_parallel_literal_inputs.rs"]
mod test_parallel_literal_inputs;
#[path = "../test_parallel_result_metadata.rs"]
mod test_parallel_result_metadata;
#[path = "../test_parallel_subworkflows_node.rs"]
mod test_parallel_subworkflows_node;
#[path = "../test_process_cancellation.rs"]
mod test_process_cancellation;
#[path = "../test_repeat_subworkflow_node.rs"]
mod test_repeat_subworkflow_node;
#[path = "../test_reused_handlers.rs"]
mod test_reused_handlers;
#[path = "../test_runtime_limit_config.rs"]
mod test_runtime_limit_config;
#[path = "../test_schema_reference_safety.rs"]
mod test_schema_reference_safety;
#[path = "../test_shell_failure_contract.rs"]
mod test_shell_failure_contract;
#[path = "../test_step_timeout.rs"]
mod test_step_timeout;
#[path = "../test_structured_node_failure.rs"]
mod test_structured_node_failure;
#[path = "../test_subworkflow_node.rs"]
mod test_subworkflow_node;
#[path = "../test_tool_dispatch_node.rs"]
mod test_tool_dispatch_node;
#[path = "../test_tool_input_isolation.rs"]
mod test_tool_input_isolation;
#[path = "../test_types.rs"]
mod test_types;
#[path = "../test_yaml_compatibility.rs"]
mod test_yaml_compatibility;
