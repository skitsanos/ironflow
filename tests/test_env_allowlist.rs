// IF-052(b): when IRONFLOW_ENV_ALLOWLIST is set, the Lua `env()` global only
// exposes the listed variables; when unset, any variable is readable (default).
//
// Dedicated test binary: it mutates process-global env vars.

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;

#[tokio::test]
async fn env_allowlist_restricts_readable_variables() {
    unsafe {
        std::env::set_var("IF052_ALLOWED_VAR", "visible");
        std::env::set_var("IF052_SECRET_VAR", "s3cr3t");
    }

    let reg = NodeRegistry::with_builtins();
    let node = reg.get("code").unwrap();
    let config = serde_json::json!({
        "source": "return { allowed = env('IF052_ALLOWED_VAR'), secret = env('IF052_SECRET_VAR') }"
    });

    // With an allowlist that names only the allowed var, the secret is nil
    // (absent from the returned table).
    unsafe {
        std::env::set_var("IRONFLOW_ENV_ALLOWLIST", "IF052_ALLOWED_VAR, OTHER");
    }
    let restricted = node.execute(&config, &Context::new()).await.unwrap();
    assert_eq!(
        restricted.get("allowed"),
        Some(&serde_json::json!("visible"))
    );
    assert!(
        !restricted.contains_key("secret"),
        "a non-allowlisted variable must not be readable: {restricted:?}"
    );

    // Without an allowlist, both variables are readable (documented default).
    unsafe {
        std::env::remove_var("IRONFLOW_ENV_ALLOWLIST");
    }
    let unrestricted = node.execute(&config, &Context::new()).await.unwrap();
    assert_eq!(
        unrestricted.get("allowed"),
        Some(&serde_json::json!("visible"))
    );
    assert_eq!(
        unrestricted.get("secret"),
        Some(&serde_json::json!("s3cr3t"))
    );

    unsafe {
        std::env::remove_var("IF052_ALLOWED_VAR");
        std::env::remove_var("IF052_SECRET_VAR");
    }
}
