//! Redacts the value of every `key=value` and `key: value` assignment whose
//! key names a credential.
//!
//! The text is read once, word by word, so the cost is linear in its length
//! (IF-177). A word is a run of ASCII letters, digits, `_` and `-`, and it is
//! a sensitive key when it ends with one of [`SENSITIVE_KEY_SUFFIXES`]. That
//! covers compound names such as `db_password`, `PGPASSWORD`, `X-API-Key` and
//! `Proxy-Authorization` without a list of every compound, and leaves
//! `token_type`, `max_tokens` and `password_policy` alone.

use super::REDACTED;
use super::values::assigned_value;

/// Key endings that name a credential, in lower case. A `_` also matches `-`
/// or nothing, so `api_key` covers `api-key`, `apikey` and `apiKey`.
const SENSITIVE_KEY_SUFFIXES: &[&str] = &[
    "access_key",
    "api_key",
    "authorization",
    "credential",
    "credentials",
    "passphrase",
    "passwd",
    "password",
    "private_key",
    "pwd",
    "secret",
    "secret_key",
    "signature",
    "token",
];

pub(super) fn redact_sensitive_assignments(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut output = String::with_capacity(text.len());
    // `text[..copied]` is already in `output`; `cursor` is the scan position.
    let mut copied = 0;
    let mut cursor = 0;
    while cursor < bytes.len() {
        if !is_key_byte(bytes[cursor]) {
            cursor += 1;
            continue;
        }
        let key_start = cursor;
        let key_end = key_start
            + bytes[key_start..]
                .iter()
                .take_while(|byte| is_key_byte(**byte))
                .count();
        cursor = key_end;
        if !is_sensitive_key(&bytes[key_start..key_end]) {
            continue;
        }
        let Some(value) = assigned_value(text, key_end) else {
            continue;
        };
        output.push_str(&text[copied..value.start]);
        output.push_str(REDACTED);
        copied = value.end;
        cursor = value.end;
    }
    output.push_str(&text[copied..]);
    output
}

fn is_sensitive_key(word: &[u8]) -> bool {
    SENSITIVE_KEY_SUFFIXES
        .iter()
        .any(|suffix| ends_with_suffix(word, suffix.as_bytes()))
}

/// Whether `word` ends with `suffix`, ignoring ASCII case. A `_` in the
/// suffix matches `_`, `-` or nothing in the word.
fn ends_with_suffix(word: &[u8], suffix: &[u8]) -> bool {
    let mut rest = word;
    for expected in suffix.iter().rev() {
        let last = rest.split_last();
        if *expected == b'_' {
            if let Some((b'_' | b'-', head)) = last {
                rest = head;
            }
            continue;
        }
        match last {
            Some((actual, head)) if actual.eq_ignore_ascii_case(expected) => rest = head,
            _ => return false,
        }
    }
    true
}

/// Key bytes are ASCII, so a word starts and ends on a character boundary.
fn is_key_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-')
}
