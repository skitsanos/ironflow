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
const UTF16_LE_MARK: &[u8] = b"\xFF\xFE";
const UTF8_MARK: &[u8] = b"\xEF\xBB\xBF";
const ESCAPE: char = '\u{1b}';
const ESCAPE_BYTE: u8 = 0x1B;
const ESCAPE_UNIT: u16 = 0x1B;

/// Decode a PDF text string into trimmed text without control characters.
///
/// The encodings are PDFDocEncoding, UTF-16BE and UTF-8 with a byte-order
/// mark, and UTF-16LE with a byte-order mark, which the format does not
/// define but some producers write. Returns `None` when the object is not a
/// string or nothing remains. An ill-formed UTF-16 or UTF-8 sequence becomes
/// U+FFFD rather than an error, so one damaged field does not hide the rest
/// of the metadata.
pub(crate) fn decode_info_string(object: &lopdf::Object) -> Option<String> {
    let bytes = object.as_str().ok()?;
    let decoded = if let Some(units) = bytes.strip_prefix(UTF16_BE_MARK) {
        decode_utf16(units, u16::from_be_bytes)
    } else if let Some(units) = bytes.strip_prefix(UTF16_LE_MARK) {
        decode_utf16(units, u16::from_le_bytes)
    } else if let Some(text) = bytes.strip_prefix(UTF8_MARK) {
        String::from_utf8_lossy(text).into_owned()
    } else {
        decode_pdf_doc(bytes)
    };
    let cleaned = strip_controls(&decoded);
    let trimmed = cleaned.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

/// UTF-16 code units in the given byte order. An odd trailing byte is not a
/// whole code unit and is ignored. A language tag written as raw code bytes
/// is removed here; one written a letter per code unit decodes to text and is
/// removed with the control characters.
fn decode_utf16(bytes: &[u8], unit: fn([u8; 2]) -> u16) -> String {
    let (pairs, _) = bytes.as_chunks::<2>();
    let mut units = Vec::with_capacity(pairs.len());
    let mut index = 0;
    while let Some(pair) = pairs.get(index) {
        index += 1;
        let value = unit(*pair);
        if value == ESCAPE_UNIT
            && let Some(length) = packed_language_tag(&pairs[index..], unit)
        {
            index += length + 1;
        } else {
            units.push(value);
        }
    }
    String::from_utf16_lossy(&units)
}

/// `rest` follows an escape code unit. A language tag whose two or four
/// letters are stored as raw bytes fills one or two code units (`en` is the
/// single unit 0x656E) before the closing escape. Returns the tag's units.
fn packed_language_tag(rest: &[[u8; 2]], unit: fn([u8; 2]) -> u16) -> Option<usize> {
    let end = rest
        .iter()
        .take(3)
        .position(|pair| unit(*pair) == ESCAPE_UNIT)?;
    let letters = rest[..end].iter().flatten().all(u8::is_ascii_alphabetic);
    (end > 0 && letters).then_some(end)
}

/// A PDFDocEncoding string. Byte 0x1B is the dot accent there, but a
/// producer may still write the language tag of the Unicode form (0x1B, two
/// to five letters, 0x1B); that sequence is removed before decoding.
fn decode_pdf_doc(bytes: &[u8]) -> String {
    let mut text = Vec::with_capacity(bytes.len());
    let mut rest = bytes;
    while let Some((byte, tail)) = rest.split_first() {
        rest = tail;
        if *byte == ESCAPE_BYTE
            && let Some(end) = tail.iter().take(6).position(|byte| *byte == ESCAPE_BYTE)
            && is_language_tag(&tail[..end])
        {
            rest = &tail[end + 1..];
        } else {
            text.push(*byte);
        }
    }
    let object = lopdf::Object::String(text, lopdf::StringFormat::Literal);
    lopdf::decode_text_string(&object)
        .unwrap_or_else(|_| String::from_utf8_lossy(object.as_str().unwrap_or_default()).into())
}

fn is_language_tag(tag: &[u8]) -> bool {
    (2..=5).contains(&tag.len()) && tag.iter().all(u8::is_ascii_alphabetic)
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
    if is_language_tag(&rest.as_bytes()[..end]) {
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

    /// IF-222 (d). UTF-16LE is outside the format but written by some
    /// producers; a language tag may be stored as raw code bytes or appear in
    /// a PDFDocEncoding string.
    #[test]
    fn decodes_little_endian_marks_and_every_language_tag_form() {
        let mut little = vec![0xFF, 0xFE];
        little.extend("Müller 報告 😀".encode_utf16().flat_map(u16::to_le_bytes));
        assert_eq!(text(&little).unwrap(), "Müller 報告 😀");
        assert_eq!(text(b"\xFF\xFEA\x00\x00").unwrap(), "A");
        // A lone low surrogate, little-endian.
        assert_eq!(text(b"\xFF\xFE\x00\xDCA\x00").unwrap(), "\u{fffd}A");

        // Raw code bytes: `en` and `enUS` between two escape code units.
        assert_eq!(text(b"\xFE\xFF\x00\x1Ben\x00\x1B\x00H\x00i").unwrap(), "Hi");
        assert_eq!(text(b"\xFE\xFF\x00\x1BenUS\x00\x1B\x00H").unwrap(), "H");
        assert_eq!(text(b"\xFF\xFE\x1B\x00enUS\x1B\x00H\x00").unwrap(), "H");
        // A letter per code unit, little-endian.
        let mut tagged = vec![0xFF, 0xFE];
        tagged.extend(
            "\u{1b}de\u{1b}Größe"
                .encode_utf16()
                .flat_map(u16::to_le_bytes),
        );
        assert_eq!(text(&tagged).unwrap(), "Größe");
        // Not a tag: three packed units, or bytes that are not letters.
        assert_eq!(
            text(b"\xFE\xFF\x00\x1BenUSab\x00\x1B\x00H").unwrap(),
            "\u{656e}\u{5553}\u{6162}H"
        );
        assert_eq!(
            text(b"\xFE\xFF\x00\x1B\x4E\x2D\x00\x1B\x00H").unwrap(),
            "\u{4e2d}H"
        );
        assert_eq!(text(b"\xFE\xFF\x00\x1B\x00\x1B\x00H").unwrap(), "H");

        // PDFDocEncoding: the tag is removed, a lone 0x1B is the dot accent.
        assert_eq!(text(b"\x1Bde\x1BGr\xF6\xDFe").unwrap(), "Größe");
        assert_eq!(text(b"\x1BenUS\x1BTagged").unwrap(), "Tagged");
        assert_eq!(text(b"a\x1Bb c\x1Bd").unwrap(), "a\u{2d9}b c\u{2d9}d");
        assert_eq!(text(b"x\x1Be\x1By").unwrap(), "x\u{2d9}e\u{2d9}y");
        assert_eq!(text(b"\x1Btoolong\x1B!").unwrap(), "\u{2d9}toolong\u{2d9}!");
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
