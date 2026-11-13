//! Decoding of PDF Info dictionary text strings, shared by `pdf_metadata`
//! and `extract_pdf`.

/// Info dictionary keys and the metadata labels they are reported under.
pub(crate) const INFO_FIELDS: [(&[u8], &str); 8] = [
    (b"Title", "title"),
    (b"Author", "author"),
    (b"Subject", "subject"),
    (b"Keywords", "keywords"),
    (b"Creator", "creator"),
    (b"Producer", "producer"),
    (b"CreationDate", "created"),
    (b"ModDate", "modified"),
];

const UTF16_BE_MARK: &[u8] = b"\xFE\xFF";
const UTF8_MARK: &[u8] = b"\xEF\xBB\xBF";
const ESCAPE: char = '\u{1b}';

/// Decode a PDF text string (PDFDocEncoding, UTF-16BE or UTF-8 with a
/// byte-order mark) into trimmed text without control characters.
///
/// Returns `None` when the object is not a string or nothing remains. An
/// ill-formed UTF-16 or UTF-8 sequence becomes U+FFFD rather than an error,
/// so one damaged field does not hide the rest of the metadata.
pub(crate) fn decode_info_string(object: &lopdf::Object) -> Option<String> {
    let bytes = object.as_str().ok()?;
    let decoded = lopdf::decode_text_string(object).unwrap_or_else(|_| decode_lossy(bytes));
    let cleaned = strip_controls(&decoded);
    let trimmed = cleaned.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn decode_lossy(bytes: &[u8]) -> String {
    if let Some(units) = bytes.strip_prefix(UTF16_BE_MARK) {
        // An odd trailing byte is not a whole code unit and is ignored.
        let (pairs, _) = units.as_chunks::<2>();
        let units = pairs
            .iter()
            .map(|pair| u16::from_be_bytes(*pair))
            .collect::<Vec<_>>();
        String::from_utf16_lossy(&units)
    } else {
        let bytes = bytes.strip_prefix(UTF8_MARK).unwrap_or(bytes);
        String::from_utf8_lossy(bytes).into_owned()
    }
}

/// Drop language-tag escapes (`ESC`, two to five letters, `ESC`) and every
/// control character other than tab, line feed and carriage return. Unicode
/// text strings may carry NUL terminators and escapes that are not content.
fn strip_controls(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(character) = rest.chars().next() {
        rest = &rest[character.len_utf8()..];
        if character == ESCAPE {
            rest = skip_language_tag(rest);
        } else if !character.is_control() || matches!(character, '\t' | '\n' | '\r') {
            output.push(character);
        }
    }
    output
}

/// `rest` follows an escape character. Skip a language tag and its closing
/// escape when one follows; otherwise leave the text untouched.
fn skip_language_tag(rest: &str) -> &str {
    let Some(end) = rest.find(ESCAPE) else {
        return rest;
    };
    let tag = &rest[..end];
    if (2..=5).contains(&tag.len()) && tag.bytes().all(|byte| byte.is_ascii_alphabetic()) {
        &rest[end + ESCAPE.len_utf8()..]
    } else {
        rest
    }
}

#[cfg(test)]
mod tests {
    use lopdf::{Object, StringFormat};

    use super::decode_info_string;

    fn text(bytes: &[u8]) -> Option<String> {
        decode_info_string(&Object::String(bytes.to_vec(), StringFormat::Hexadecimal))
    }

    fn utf16(value: &str) -> Vec<u8> {
        let mut bytes = vec![0xFE, 0xFF];
        bytes.extend(value.encode_utf16().flat_map(u16::to_be_bytes));
        bytes
    }

    #[test]
    fn decodes_every_text_string_encoding() {
        assert_eq!(text(&utf16("Müller Report")).unwrap(), "Müller Report");
        assert_eq!(text(&utf16("報告 😀")).unwrap(), "報告 😀");
        assert_eq!(text(b"\xEF\xBB\xBFGr\xC3\xB6\xC3\x9Fe").unwrap(), "Größe");
        // PDFDocEncoding: 0xE9 is e-acute, 0x80 is a bullet, 0xA0 is the euro sign.
        assert_eq!(text(b"caf\xE9 \x80 \xA0").unwrap(), "café • €");
        assert_eq!(text(b"  Plain Author \n").unwrap(), "Plain Author");
    }

    #[test]
    fn removes_controls_and_language_tags_but_keeps_line_structure() {
        assert_eq!(text(&utf16("Title\0\0")).unwrap(), "Title");
        assert_eq!(text(&utf16("A\u{7}B\u{9b}C\u{7f}")).unwrap(), "ABC");
        assert_eq!(text(&utf16("one\ttwo\nthree")).unwrap(), "one\ttwo\nthree");
        assert_eq!(text(&utf16("\u{1b}enUS\u{1b}Hello")).unwrap(), "Hello");
        assert_eq!(text(&utf16("\u{1b}de\u{1b}Größe")).unwrap(), "Größe");
        assert_eq!(text(&utf16("a\u{1b}b c\u{1b}d")).unwrap(), "ab cd");
        assert_eq!(text(&utf16("\0")), None);
        assert_eq!(text(b""), None);
    }

    #[test]
    fn ill_formed_unicode_is_replaced_instead_of_failing() {
        // A lone high surrogate, then "A".
        assert_eq!(text(b"\xFE\xFF\xD8\x00\x00A").unwrap(), "\u{fffd}A");
        // An odd trailing byte is not a code unit.
        assert_eq!(text(b"\xFE\xFF\x00A\x00").unwrap(), "A");
        assert_eq!(text(b"\xEF\xBB\xBFok\xFF").unwrap(), "ok\u{fffd}");
    }

    #[test]
    fn non_strings_are_not_metadata() {
        assert_eq!(decode_info_string(&Object::Integer(7)), None);
        assert_eq!(decode_info_string(&Object::Name(b"Title".to_vec())), None);
    }
}
