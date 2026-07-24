use crate::engine::types::Context;

/// Evaluate a simple condition expression against context.
/// Supports: ctx.key > N, ctx.key == "value", ctx.key exists, ctx.key != N
pub(super) fn evaluate_condition(condition: &str, ctx: &Context) -> bool {
    let condition = condition.trim();

    if condition.ends_with(" exists") {
        let key = condition.trim_end_matches(" exists").trim();
        return resolve_nested(strip_ctx_prefix(key), ctx).is_some();
    }

    for operator in ["==", "!=", ">=", "<=", ">", "<"] {
        if let Some(position) = condition.find(operator) {
            let left = condition[..position].trim();
            let right = condition[position + operator.len()..].trim();
            return compare_values(resolve_nested(strip_ctx_prefix(left), ctx), operator, right);
        }
    }

    match resolve_nested(strip_ctx_prefix(condition), ctx) {
        Some(serde_json::Value::Bool(value)) => *value,
        Some(serde_json::Value::Null) | None => false,
        Some(_) => true,
    }
}

pub(super) fn resolve_nested<'a>(path: &str, ctx: &'a Context) -> Option<&'a serde_json::Value> {
    let mut parts = path.split('.');
    let mut current = ctx.get(parts.next()?)?;
    for part in parts {
        current = current.get(part)?;
    }
    Some(current)
}

pub(super) fn resolve_ctx_value(expression: &str, ctx: &Context) -> String {
    match resolve_nested(strip_ctx_prefix(expression), ctx) {
        Some(serde_json::Value::String(value)) => value.clone(),
        Some(value) => value.to_string(),
        None => String::new(),
    }
}

fn strip_ctx_prefix(value: &str) -> &str {
    value.strip_prefix("ctx.").unwrap_or(value)
}

fn compare_values(left: Option<&serde_json::Value>, operator: &str, right: &str) -> bool {
    let Some(left) = left else {
        return operator == "!=";
    };

    if let Some(result) = numeric_comparison(left, operator, right) {
        return result;
    }

    let Some(left) = left.as_str() else {
        return false;
    };
    let right = right.trim_matches('"').trim_matches('\'');
    match operator {
        "==" => left == right,
        "!=" => left != right,
        _ => false,
    }
}

fn numeric_comparison(left: &serde_json::Value, operator: &str, right: &str) -> Option<bool> {
    let (left, right) = left.as_f64().zip(right.parse::<f64>().ok())?;
    Some(match operator {
        "==" => (left - right).abs() < f64::EPSILON,
        "!=" => (left - right).abs() >= f64::EPSILON,
        ">" => left > right,
        "<" => left < right,
        ">=" => left >= right,
        "<=" => left <= right,
        _ => false,
    })
}
