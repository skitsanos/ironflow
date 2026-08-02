use std::ffi::OsString;

use ironflow::lua::runtime::LuaRuntime;
use ironflow::nodes::NodeRegistry;

const LIMIT_ENV: &str = "IRONFLOW_MAX_FLOW_SOURCE_BYTES";

struct EnvironmentGuard {
    previous: Option<OsString>,
}

impl EnvironmentGuard {
    fn set(value: &str) -> Self {
        let previous = std::env::var_os(LIMIT_ENV);
        unsafe { std::env::set_var(LIMIT_ENV, value) };
        Self { previous }
    }
}

impl Drop for EnvironmentGuard {
    fn drop(&mut self) {
        unsafe {
            match self.previous.take() {
                Some(value) => std::env::set_var(LIMIT_ENV, value),
                None => std::env::remove_var(LIMIT_ENV),
            }
        }
    }
}

#[test]
fn flow_loaders_enforce_the_configured_source_limit() {
    let _environment = EnvironmentGuard::set("128");
    let registry = NodeRegistry::new();
    let valid = "local flow = Flow.new('small')\nreturn flow";

    let inline_flow = LuaRuntime::load_flow_from_string(valid, &registry).unwrap();
    assert_eq!(inline_flow.name, "small");

    let oversized = format!("--{}\n{valid}", "x".repeat(128));
    let inline_error = LuaRuntime::load_flow_from_string(&oversized, &registry).unwrap_err();
    assert!(
        format!("{inline_error:#}").contains(LIMIT_ENV),
        "{inline_error:#}"
    );

    let directory = tempfile::tempdir().unwrap();
    let oversized_path = directory.path().join("oversized.lua");
    std::fs::write(&oversized_path, oversized).unwrap();
    let file_error =
        LuaRuntime::load_flow(oversized_path.to_str().unwrap(), &registry).unwrap_err();
    assert!(
        format!("{file_error:#}").contains(LIMIT_ENV),
        "{file_error:#}"
    );

    unsafe { std::env::set_var(LIMIT_ENV, "0") };
    assert_eq!(ironflow::util::limits::max_flow_source_bytes(), 1024 * 1024);

    unsafe { std::env::set_var(LIMIT_ENV, "invalid") };
    assert_eq!(ironflow::util::limits::max_flow_source_bytes(), 1024 * 1024);
}
