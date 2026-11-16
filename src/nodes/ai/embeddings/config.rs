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

/// The configured `provider` name, or `default` when the key is absent.
///
/// A value that is not a string is an error, never the default: the provider
/// decides which endpoint receives the input and the credential.
pub(in crate::nodes::ai) fn provider_name<'a>(
    config: &'a serde_json::Value,
    node: &str,
    default: &'a str,
) -> anyhow::Result<&'a str> {
    match config.get("provider") {
        None | Some(serde_json::Value::Null) => Ok(default),
        Some(serde_json::Value::String(name)) => Ok(name),
        Some(_) => anyhow::bail!("{node}: 'provider' must be a string"),
    }
}
