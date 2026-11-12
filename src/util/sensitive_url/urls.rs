use url::Url;

use super::search::AsciiNeedles;
use super::{REDACTED, REDACTED_URL};

const SUPPORTED_SCHEMES: &[&str] = &[
    "http",
    "https",
    "postgres",
    "postgresql",
    "redis",
    "rediss",
    "smtp",
    "smtps",
    "sqlite",
];

static URL_PREFIXES: AsciiNeedles = AsciiNeedles::new(&[
    "postgresql://",
    "postgres://",
    "rediss://",
    "redis://",
    "https://",
    "http://",
    "smtps://",
    "smtp://",
    "sqlite:",
]);

pub(super) fn redact_url(raw: &str, hide_path: bool) -> String {
    if raw == REDACTED_URL || raw.chars().any(char::is_control) || !has_valid_percent_encoding(raw)
    {
        return REDACTED_URL.to_string();
    }
    let Ok(mut parsed) = Url::parse(raw) else {
        return REDACTED_URL.to_string();
    };
    if !SUPPORTED_SCHEMES.contains(&parsed.scheme()) {
        return REDACTED_URL.to_string();
    }
    if parsed.password().is_some() && parsed.set_password(None).is_err() {
        return REDACTED_URL.to_string();
    }
    if !parsed.username().is_empty() && parsed.set_username("").is_err() {
        return REDACTED_URL.to_string();
    }
    if parsed.query().is_some() {
        let keys = parsed
            .query_pairs()
            .map(|(key, _)| key.into_owned())
            .collect::<Vec<_>>();
        parsed.set_query(None);
        if !keys.is_empty() {
            let mut query = parsed.query_pairs_mut();
            for key in keys {
                query.append_pair(&key, REDACTED);
            }
        }
    }
    parsed.set_fragment(None);
    if hide_path {
        parsed.set_path(&format!("/{REDACTED}"));
    }
    parsed.to_string()
}

/// Redact every URL span in `text`. Each search resumes after the previous
/// span, so the text is read once.
///
/// `truncated` says that `text` is a prefix of a longer diagnostic. A span
/// that reaches the cut may have lost the `@` that separates user information
/// from the host, and would then parse as a harmless `host:port`; it is
/// replaced whole instead.
pub(super) fn redact_url_spans(text: &str, truncated: bool) -> String {
    let mut output = String::with_capacity(text.len());
    let mut cursor = 0;
    while let Some((relative_start, prefix)) = URL_PREFIXES.find_first(&text[cursor..]) {
        let start = cursor + relative_start;
        output.push_str(&text[cursor..start]);
        let end = text[start..]
            .char_indices()
            .find_map(|(offset, character)| {
                (offset > 0
                    && (character.is_whitespace()
                        || character.is_control()
                        || matches!(character, '\'' | '"' | '<' | '>')))
                .then_some(start + offset)
            })
            .unwrap_or(text.len());
        let raw_url = &text[start..end];
        if truncated && end == text.len() {
            output.push_str(REDACTED_URL);
        } else if prefix.eq_ignore_ascii_case("http://") || prefix.eq_ignore_ascii_case("https://")
        {
            output.push_str(&redact_url(raw_url, true));
        } else {
            output.push_str(&redact_url(raw_url, false));
        }
        cursor = end;
    }
    output.push_str(&text[cursor..]);
    output
}

fn has_valid_percent_encoding(raw: &str) -> bool {
    let bytes = raw.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if bytes
                .get(index + 1..=index + 2)
                .is_none_or(|escape| !escape.iter().all(u8::is_ascii_hexdigit))
            {
                return false;
            }
            index += 3;
        } else {
            index += 1;
        }
    }
    true
}
