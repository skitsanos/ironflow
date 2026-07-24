use crate::engine::types::Context;
use crate::lua::interpolate::interpolate_ctx;

/// Resolve a config string parameter, falling back to an environment variable.
pub(in crate::nodes::ai) fn resolve_param(
    config: &serde_json::Value,
    key: &str,
    env_key: &str,
    ctx: &Context,
) -> Option<String> {
    config
        .get(key)
        .and_then(|value| value.as_str())
        .map(|value| interpolate_ctx(value, ctx))
        .or_else(|| std::env::var(env_key).ok())
}
