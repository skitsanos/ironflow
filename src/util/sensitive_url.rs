mod assignments;
mod search;
mod urls;

use std::fmt;

const REDACTED: &str = "[REDACTED]";
const REDACTED_URL: &str = "[REDACTED URL]";

const RAW_URL_ERROR_LABELS: &[&str] = &[
    "Invalid Redis URL:",
    "Failed to connect to Redis at",
    "Failed to connect SQL state store at",
    "Failed to connect SQL event store at",
];

/// Redacted display for a database, cache, or SMTP connection URL.
///
/// The raw URL is retained only by reference and is never exposed through
/// `Display` or `Debug`.
pub(crate) struct Connection<'a> {
    raw: &'a str,
}

impl<'a> Connection<'a> {
    pub(crate) fn new(raw: &'a str) -> Self {
        Self { raw }
    }
}

impl fmt::Display for Connection<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&redact_connection_url(self.raw))
    }
}

impl fmt::Debug for Connection<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, formatter)
    }
}

/// Redacted display for endpoints whose path is itself a credential, such as
/// an incoming webhook URL.
pub(crate) struct SecretEndpoint<'a> {
    raw: &'a str,
}

impl<'a> SecretEndpoint<'a> {
    pub(crate) fn new(raw: &'a str) -> Self {
        Self { raw }
    }
}

impl fmt::Display for SecretEndpoint<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&redact_secret_endpoint(self.raw))
    }
}

impl fmt::Debug for SecretEndpoint<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, formatter)
    }
}

pub(crate) fn redact_connection_url(raw: &str) -> String {
    urls::redact_url(raw, false)
}

pub(crate) fn redact_secret_endpoint(raw: &str) -> String {
    urls::redact_url(raw, true)
}

/// Scrub URL spans and common DSN-style credential assignments from an error
/// string. This is a final sink defense; call sites that own a URL should use
/// `Connection` or `SecretEndpoint` before constructing the error.
pub fn redact_sensitive_text(text: &str) -> String {
    let text = redact_known_raw_url_errors(text);
    let text = urls::redact_url_spans(&text);
    let text = assignments::redact_sensitive_assignments(&text);
    text.chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect()
}

fn redact_known_raw_url_errors(text: &str) -> String {
    let Some((index, label)) =
        search::find_first_ascii_case_insensitive(text, RAW_URL_ERROR_LABELS)
    else {
        return text.to_string();
    };
    let value_start = index + label.len();
    format!("{} {REDACTED_URL}", text[..value_start].trim_end())
}

#[cfg(test)]
#[path = "sensitive_url/tests.rs"]
mod tests;
