use serde_json::Value;

use crate::engine::types::Context;

use super::InterpolationError;
use super::path;

pub(super) fn render(template: &str, context: &Context) -> Result<String, InterpolationError> {
    visit(template, |expression| {
        let segments = path::parse(expression)?;
        Ok(stringify(path::resolve(&segments, context)))
    })
}

pub(super) fn validate(template: &str) -> Result<(), InterpolationError> {
    visit(template, |expression| {
        path::parse(expression)?;
        Ok(String::new())
    })
    .map(|_| ())
}

fn visit<F>(template: &str, mut resolve: F) -> Result<String, InterpolationError>
where
    F: FnMut(&str) -> Result<String, InterpolationError>,
{
    let mut output = String::with_capacity(template.len());
    let mut cursor = 0;

    while let Some(relative_open) = template[cursor..].find("${") {
        let open = cursor + relative_open;
        let close = find_close(template, open + 2);
        let body_end = close.unwrap_or(template.len());
        let expression = &template[open + 2..body_end];
        let reserved = is_reserved(expression);
        let escaped = reserved && has_escape_backslash(template, open);

        if escaped {
            output.push_str(&template[cursor..open - 1]);
            output.push_str(&template[open..close.map_or(template.len(), |index| index + 1)]);
            cursor = close.map_or(template.len(), |index| index + 1);
            continue;
        }

        output.push_str(&template[cursor..open]);
        let Some(close) = close else {
            if reserved {
                return Err(InterpolationError::new(
                    &template[open..],
                    "the placeholder is missing its closing `}`",
                ));
            }
            output.push_str(&template[open..]);
            cursor = template.len();
            break;
        };

        if reserved {
            output.push_str(&resolve(expression)?);
        } else {
            output.push_str(&template[open..=close]);
        }
        cursor = close + 1;
    }

    output.push_str(&template[cursor..]);
    Ok(output)
}

fn find_close(template: &str, mut cursor: usize) -> Option<usize> {
    let bytes = template.as_bytes();
    let mut in_string = false;
    let mut escaped = false;

    while cursor < bytes.len() {
        match bytes[cursor] {
            b'"' if !escaped => in_string = !in_string,
            b'\\' if in_string && !escaped => {
                escaped = true;
                cursor += 1;
                continue;
            }
            b'}' if !in_string => return Some(cursor),
            _ => {}
        }
        escaped = false;
        cursor += 1;
    }
    None
}

fn is_reserved(expression: &str) -> bool {
    expression == "ctx" || expression.starts_with("ctx.") || expression.starts_with("ctx[")
}

fn has_escape_backslash(template: &str, open: usize) -> bool {
    let preceding = template.as_bytes()[..open]
        .iter()
        .rev()
        .take_while(|&&byte| byte == b'\\')
        .count();
    preceding % 2 == 1
}

fn stringify(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(value)) => value.clone(),
        Some(Value::Null) | None => String::new(),
        Some(value) => value.to_string(),
    }
}
