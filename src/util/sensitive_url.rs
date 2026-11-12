mod assignments;
mod search;
mod urls;
mod values;

use std::borrow::Cow;
use std::fmt;

use search::AsciiNeedles;

const REDACTED: &str = "[REDACTED]";
const REDACTED_URL: &str = "[REDACTED URL]";
const TRUNCATED: &str = "...[truncated]";

/// Longest prefix of a diagnostic that [`redact_sensitive_text`] scans and
/// keeps. A diagnostic is read by a person; megabytes of provider body or
/// script output are not, and scanning them would hold an async worker.
pub const MAX_DIAGNOSTIC_BYTES: usize = 64 * 1024;

static RAW_URL_ERROR_LABELS: AsciiNeedles = AsciiNeedles::new(&[
    "Invalid Redis URL:",
    "Failed to connect to Redis at",
    "Failed to connect SQL state store at",
    "Failed to connect SQL event store at",
]);

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

/// Scrub URL spans and credential assignments from a diagnostic string.
///
/// This is a final sink defense for text that may reach logs, events, stored
/// task state or an API response; call sites that own a URL should use
/// `Connection` or `SecretEndpoint` before constructing the error. The shapes
/// it guarantees are listed in `docs/SECURITY.md`.
///
/// The work is bounded (IF-177): only the first [`MAX_DIAGNOSTIC_BYTES`] are
/// scanned and kept, in time linear in that prefix, and the rest is replaced
/// by a truncation marker. Redact exact values first: a value cut by the
/// limit no longer matches as a whole.
pub fn redact_sensitive_text(text: &str) -> String {
    let (text, truncated) = bounded_prefix(text);
    let mut redacted = scrub(text, truncated);
    if truncated {
        redacted.push_str(TRUNCATED);
    }
    redacted
}

fn bounded_prefix(text: &str) -> (&str, bool) {
    if text.len() <= MAX_DIAGNOSTIC_BYTES {
        return (text, false);
    }
    let mut end = MAX_DIAGNOSTIC_BYTES;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    (&text[..end], true)
}

/// Every pass reads its input once. `truncated` marks `text` as a prefix of
/// a longer diagnostic, so a URL at its end is not trusted to be complete.
fn scrub(text: &str, truncated: bool) -> String {
    let text = redact_known_raw_url_errors(text);
    let text = urls::redact_url_spans(&text, truncated);
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

fn redact_known_raw_url_errors(text: &str) -> Cow<'_, str> {
    let Some((index, label)) = RAW_URL_ERROR_LABELS.find_first(text) else {
        return Cow::Borrowed(text);
    };
    let value_start = index + label.len();
    Cow::Owned(format!("{} {REDACTED_URL}", text[..value_start].trim_end()))
}

#[cfg(test)]
mod cost_tests;
#[cfg(test)]
mod shape_tests;
#[cfg(test)]
#[path = "sensitive_url/tests.rs"]
mod tests;
