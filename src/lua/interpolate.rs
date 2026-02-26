use crate::engine::types::Context;

/// Interpolate `${ctx.key}` and `${ctx.nested.key}` patterns in a string.
pub fn interpolate_ctx(template: &str, ctx: &Context) -> String {
    let mut result = template.to_string();
    let mut start = 0;

    loop {
        let open = match result[start..].find("${ctx.") {
            Some(pos) => start + pos,
            None => break,
        };

        let close = match result[open..].find('}') {
            Some(pos) => open + pos,
            None => break,
        };

        let path = &result[open + 6..close]; // skip "${ctx."
        let value = resolve_path(path, ctx);

        result.replace_range(open..=close, &value);
        start = open + value.len();
    }

    result
}

/// Resolve a dotted path (e.g., "user.email") from context.
fn resolve_path(path: &str, ctx: &Context) -> String {
    let parts: Vec<&str> = path.split('.').collect();

    if parts.is_empty() {
        return String::new();
    }

    let first = match ctx.get(parts[0]) {
        Some(v) => v,
        None => return String::new(),
    };

    let mut current = first;
    for part in &parts[1..] {
        current = match current.get(part) {
            Some(v) => v,
            None => return String::new(),
        };
    }

    match current {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_simple_interpolation() {
        let mut ctx = HashMap::new();
        ctx.insert(
            "name".to_string(),
            serde_json::Value::String("Alice".to_string()),
        );

        assert_eq!(interpolate_ctx("Hello ${ctx.name}!", &ctx), "Hello Alice!");
    }

    #[test]
    fn test_nested_interpolation() {
        let mut ctx = HashMap::new();
        ctx.insert(
            "user".to_string(),
            serde_json::json!({"email": "alice@example.com"}),
        );

        assert_eq!(
            interpolate_ctx("Email: ${ctx.user.email}", &ctx),
            "Email: alice@example.com"
        );
    }

    #[test]
    fn test_no_interpolation() {
        let ctx = HashMap::new();
        assert_eq!(interpolate_ctx("plain text", &ctx), "plain text");
    }

    #[test]
    fn test_missing_key() {
        let ctx = HashMap::new();
        assert_eq!(interpolate_ctx("Hello ${ctx.missing}!", &ctx), "Hello !");
    }
}
