use anyhow::{Result, bail};

pub(super) fn positive_index(number: f64, path: &str) -> Result<usize> {
    if !number.is_finite() || number.fract() != 0.0 || number < 1.0 || number >= usize::MAX as f64 {
        bail!(
            "invalid Lua array index {number} at {path}; indices must be positive consecutive integers starting at 1"
        );
    }
    Ok(number as usize)
}

pub(super) fn positive_integer_index(integer: i64, path: &str) -> Result<usize> {
    if integer < 1 {
        bail!(
            "invalid Lua array index {integer} at {path}; indices must be positive consecutive integers starting at 1"
        );
    }
    usize::try_from(integer).map_err(|_| {
        anyhow::anyhow!(
            "Lua array index {integer} at {path} exceeds this platform's supported range"
        )
    })
}

pub(super) fn json_field_path(parent: &str, key: &str) -> String {
    let mut chars = key.chars();
    let identifier = chars
        .next()
        .is_some_and(|first| first == '_' || first.is_ascii_alphabetic())
        && chars.all(|character| character == '_' || character.is_ascii_alphanumeric());

    if identifier {
        format!("{parent}.{key}")
    } else {
        let quoted = serde_json::to_string(key).unwrap_or_else(|_| "\"<invalid>\"".to_string());
        format!("{parent}[{quoted}]")
    }
}
