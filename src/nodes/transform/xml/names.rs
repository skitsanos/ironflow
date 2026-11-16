//! XML name and character rules for `xml_stringify` (IF-196).
//!
//! JSON keys become element and attribute names, so each one must be a name
//! before it is written as markup: an XML 1.0 `Name` that is also a namespace
//! `QName`, which means either one name without a colon or `prefix:local`.

use anyhow::Result;

#[derive(Clone, Copy)]
pub(super) enum NameKind {
    Element,
    Attribute,
}

impl NameKind {
    fn label(self) -> &'static str {
        match self {
            Self::Element => "element",
            Self::Attribute => "attribute",
        }
    }
}

/// Checks `name` and returns it, or an error that says where it came from.
///
/// `xmlns` and the `xmlns:` prefix declare namespaces, so they are attribute
/// names only, and `xmlns:xmlns` is never valid. The `xml:` prefix is bound by
/// definition and is accepted. An unprefixed name that merely starts with
/// `xml` (`xml_data`) is accepted, as parsers accept it.
pub(super) fn checked_name<'a>(name: &'a str, kind: NameKind, origin: &str) -> Result<&'a str> {
    let valid = match name.split_once(':') {
        None => is_ncname(name) && (name != "xmlns" || matches!(kind, NameKind::Attribute)),
        Some((prefix, local)) => {
            is_ncname(prefix)
                && is_ncname(local)
                && (prefix != "xmlns" || (matches!(kind, NameKind::Attribute) && local != "xmlns"))
        }
    };
    anyhow::ensure!(
        valid,
        "xml_stringify: {origin} {} is not a valid XML {} name",
        quoted(name),
        kind.label()
    );
    Ok(name)
}

/// Rejects text that XML 1.0 cannot carry even as a character reference.
pub(super) fn checked_text<'a>(text: &'a str, origin: &str) -> Result<&'a str> {
    let invalid = text.chars().find(|character| {
        !matches!(character,
            '\t' | '\n' | '\r' | ' '..='\u{d7ff}' | '\u{e000}'..='\u{fffd}' | '\u{10000}'..)
    });
    if let Some(character) = invalid {
        anyhow::bail!(
            "xml_stringify: {origin} contains character U+{:04X}, which XML 1.0 does not allow",
            u32::from(character)
        );
    }
    Ok(text)
}

/// A bounded, escaped rendering of an untrusted key for error messages.
pub(super) fn quoted(name: &str) -> String {
    const LIMIT: usize = 48;
    if name.chars().count() <= LIMIT {
        return format!("{name:?}");
    }
    let head: String = name.chars().take(LIMIT).collect();
    format!("{head:?}...")
}

fn is_ncname(name: &str) -> bool {
    let mut characters = name.chars();
    characters.next().is_some_and(is_name_start) && characters.all(is_name_char)
}

fn is_name_start(character: char) -> bool {
    matches!(character,
        'A'..='Z' | '_' | 'a'..='z'
        | '\u{c0}'..='\u{d6}' | '\u{d8}'..='\u{f6}' | '\u{f8}'..='\u{2ff}'
        | '\u{370}'..='\u{37d}' | '\u{37f}'..='\u{1fff}' | '\u{200c}'..='\u{200d}'
        | '\u{2070}'..='\u{218f}' | '\u{2c00}'..='\u{2fef}' | '\u{3001}'..='\u{d7ff}'
        | '\u{f900}'..='\u{fdcf}' | '\u{fdf0}'..='\u{fffd}' | '\u{10000}'..='\u{effff}')
}

fn is_name_char(character: char) -> bool {
    is_name_start(character)
        || matches!(character,
            '-' | '.' | '0'..='9' | '\u{b7}' | '\u{300}'..='\u{36f}' | '\u{203f}'..='\u{2040}')
}
