//! Single-character classes of Lua patterns (`lstrlib.c`), in the C locale.

use mlua::prelude::*;

use super::pattern::pattern_error;

/// The pattern escape character.
pub(super) const ESC: u8 = b'%';

/// Offset after the single-character class that starts at `p`.
pub(super) fn class_end(pat: &[u8], p: usize) -> LuaResult<usize> {
    let mut p = p + 1;
    match pat[p - 1] {
        ESC => {
            if p == pat.len() {
                return Err(pattern_error("malformed pattern (ends with '%')"));
            }
            Ok(p + 1)
        }
        b'[' => {
            if pat.get(p) == Some(&b'^') {
                p += 1;
            }
            // The first character of a set is taken literally, so `[]]` is a
            // set that holds `]`.
            loop {
                let Some(&byte) = pat.get(p) else {
                    return Err(pattern_error("malformed pattern (missing ']')"));
                };
                p += 1;
                if byte == ESC && p < pat.len() {
                    p += 1;
                }
                if pat.get(p) == Some(&b']') {
                    return Ok(p + 1);
                }
            }
        }
        _ => Ok(p),
    }
}

/// Whether the subject byte at `s` matches the class `pat[p..ep]`.
pub(super) fn single_match(src: &[u8], pat: &[u8], s: usize, p: usize, ep: usize) -> bool {
    let Some(&byte) = src.get(s) else {
        return false;
    };
    match pat[p] {
        b'.' => true,
        ESC => matches_class(byte, pat[p + 1]),
        b'[' => matches_set(pat, byte, p, ep - 1),
        literal => literal == byte,
    }
}

/// Whether `byte` is in the class `%class`.
fn matches_class(byte: u8, class: u8) -> bool {
    let member = match class.to_ascii_lowercase() {
        b'a' => byte.is_ascii_alphabetic(),
        b'c' => byte.is_ascii_control(),
        b'd' => byte.is_ascii_digit(),
        b'g' => byte.is_ascii_graphic(),
        b'l' => byte.is_ascii_lowercase(),
        b'p' => byte.is_ascii_punctuation(),
        b's' => byte == b' ' || (9..=13).contains(&byte),
        b'u' => byte.is_ascii_uppercase(),
        b'w' => byte.is_ascii_alphanumeric(),
        b'x' => byte.is_ascii_hexdigit(),
        // Deprecated, but still accepted by the reference library.
        b'z' => byte == 0,
        _ => return class == byte,
    };
    member == class.is_ascii_lowercase()
}

/// Whether `byte` is in the set that starts with `[` at `p` and ends with
/// `]` at `end`.
pub(super) fn matches_set(pat: &[u8], byte: u8, p: usize, end: usize) -> bool {
    let mut p = p;
    let mut found = true;
    if pat[p + 1] == b'^' {
        found = false;
        p += 1;
    }
    loop {
        p += 1;
        if p >= end {
            return !found;
        }
        if pat[p] == ESC {
            p += 1;
            if matches_class(byte, pat[p]) {
                return found;
            }
        } else if pat[p + 1] == b'-' && p + 2 < end {
            p += 2;
            if pat[p - 2] <= byte && byte <= pat[p] {
                return found;
            }
        } else if pat[p] == byte {
            return found;
        }
    }
}
