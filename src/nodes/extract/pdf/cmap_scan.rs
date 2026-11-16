//! Cost of a ToUnicode CMap, measured before lopdf parses it (IF-178).
//!
//! lopdf builds a reverse map holding one entry per source code of every
//! `bfrange`, in one uninterruptible call, so a 1 KiB stream naming
//! `<00000001> <FFFFFFFE>` costs billions of entries. This scan is linear in
//! the stream, keeps no table, and never reports less than lopdf's grammar
//! would materialize:
//!
//! - range mode starts at every literal `beginbfrange`, even inside a comment
//!   or string, and ends only at an `endbfrange` outside a comment. A `%`
//!   always opens a comment here, so every comment lopdf sees is one here too
//!   and a commented-out terminator cannot end the section early;
//! - in range mode every `<low> <high> destination` shape is charged, wherever
//!   it appears. A shape without a comment inside is skipped as a unit, which
//!   is exactly how lopdf consumes it; one with a comment inside a destination
//!   is over budget outright;
//! - every other hexadecimal string is paired with its neighbour and charged
//!   its longer half, which covers each `bfchar` destination under either
//!   alignment, and `longest_target` is the maximum over all of them.

use anyhow::Result;

/// lopdf rejects a destination longer than this many UTF-16 units.
pub(super) const MAX_TARGET_UNITS: usize = 256;
const CHECKPOINT_BYTES: usize = 64 * 1024;
/// A range whose destination hides its extent cannot be bounded: over budget.
const UNMEASURABLE: u64 = u64::MAX;

#[derive(Debug, PartialEq, Eq)]
pub(super) struct Cost {
    /// Mapping entries lopdf materializes, each weighted by its UTF-16 units.
    pub(super) entries: u64,
    /// Longest destination of any mapping, in UTF-16 units; at least one.
    pub(super) longest_target: usize,
}

struct Target {
    units: usize,
    /// One past the closing `>`; `None` when a comment or an over-long string
    /// hides the extent.
    end: Option<usize>,
    one_line: bool,
}

struct RangeLine {
    entries: u64,
    units: usize,
    /// Where the shape ends when lopdf would consume exactly these bytes.
    end: Option<usize>,
    one_line: bool,
}

pub(super) fn scan(bytes: &[u8], mut checkpoint: impl FnMut() -> Result<()>) -> Result<Cost> {
    let mut cost = Cost {
        entries: 0,
        longest_target: 1,
    };
    let (mut in_comment, mut in_ranges) = (false, false);
    let mut unpaired = None;
    let (mut index, mut next_checkpoint) = (0, 0);
    while index < bytes.len() {
        if index >= next_checkpoint {
            checkpoint()?;
            next_checkpoint = index + CHECKPOINT_BYTES;
        }
        match bytes[index] {
            b'\r' | b'\n' => in_comment = false,
            b'%' => in_comment = true,
            b'b' if bytes[index..].starts_with(b"beginbfrange") => in_ranges = true,
            b'e' if !in_comment && bytes[index..].starts_with(b"endbfrange") => in_ranges = false,
            b'<' => {
                if in_ranges && let Some(line) = range_line(bytes, index) {
                    cost.entries = cost.entries.saturating_add(line.entries);
                    cost.longest_target = cost.longest_target.max(line.units);
                    if let Some(end) = line.end {
                        // The skipped bytes hold no `%`, so a line end among
                        // them closes any comment the shape started in.
                        in_comment &= line.one_line;
                        index = end;
                        continue;
                    }
                }
                if let Some(target) = target(bytes, index) {
                    cost.longest_target = cost.longest_target.max(target.units);
                    unpaired = match unpaired.take() {
                        Some(first) => {
                            cost.entries =
                                cost.entries.saturating_add(target.units.max(first) as u64);
                            None
                        }
                        None => Some(target.units),
                    };
                }
            }
            _ => {}
        }
        index += 1;
    }
    cost.entries = cost.entries.saturating_add(unpaired.unwrap_or(0) as u64);
    Ok(cost)
}

fn blanks(bytes: &[u8], mut index: usize) -> usize {
    while matches!(bytes.get(index), Some(b' ' | b'\t')) {
        index += 1;
    }
    index
}

/// A source code: `<`, one to four bytes as hexadecimal pairs, `>`.
fn code(bytes: &[u8], start: usize) -> Option<(u32, usize, usize)> {
    if bytes.get(start) != Some(&b'<') {
        return None;
    }
    let (mut value, mut digits) = (0_u32, 0);
    while let Some(digit) = bytes.get(start + 1 + digits).and_then(|byte| hex(*byte)) {
        if digits == 8 {
            return None;
        }
        value = (value << 4) | digit;
        digits += 1;
    }
    let end = start + 1 + digits;
    (digits >= 2 && digits % 2 == 0 && bytes.get(end) == Some(&b'>')).then_some((
        value,
        digits,
        end + 1,
    ))
}

fn hex(byte: u8) -> Option<u32> {
    char::from(byte).to_digit(16)
}

/// A destination: hexadecimal digits, optionally separated by white space.
/// lopdf also accepts comments between units. Following them would make the
/// scan quadratic, so such a destination reports the longest length lopdf
/// allows and no extent.
fn target(bytes: &[u8], start: usize) -> Option<Target> {
    let (mut digits, mut one_line) = (0_usize, true);
    let unclear = Target {
        units: MAX_TARGET_UNITS,
        end: None,
        one_line: false,
    };
    for (offset, byte) in bytes.get(start + 1..)?.iter().enumerate() {
        match byte {
            b' ' | b'\t' => {}
            b'\r' | b'\n' => one_line = false,
            b'%' => return Some(unclear),
            b'>' => {
                return (digits > 0).then_some(Target {
                    units: digits.div_ceil(4),
                    end: Some(start + offset + 2),
                    one_line,
                });
            }
            _ if hex(*byte).is_some() => {
                digits += 1;
                if digits > MAX_TARGET_UNITS * 4 {
                    return Some(unclear);
                }
            }
            _ => return None,
        }
    }
    None
}

/// `<low> <high>` of one width, then a destination or an array of them.
fn range_line(bytes: &[u8], start: usize) -> Option<RangeLine> {
    let (low, width, after_low) = code(bytes, start)?;
    let (high, high_width, after_high) = code(bytes, blanks(bytes, after_low))?;
    if width != high_width || high < low {
        return None;
    }
    let span = u64::from(high - low) + 1;
    // Skipped codes still count towards the longest hexadecimal string.
    let code_units = width.div_ceil(4);
    let destination = blanks(bytes, after_high);
    match bytes.get(destination)? {
        b'<' => {
            let target = target(bytes, destination)?;
            Some(RangeLine {
                entries: match target.end {
                    Some(_) => span.saturating_mul(target.units as u64),
                    None => UNMEASURABLE,
                },
                units: target.units.max(code_units),
                end: target.end,
                one_line: target.one_line,
            })
        }
        b'[' => array(bytes, destination + 1, span).map(|line| RangeLine {
            units: line.units.max(code_units),
            ..line
        }),
        _ => None,
    }
}

/// Destinations separated by blanks up to `]`. lopdf walks the whole span and
/// keeps every element, so the charge covers whichever is larger.
fn array(bytes: &[u8], start: usize, span: u64) -> Option<RangeLine> {
    let (mut longest, mut total, mut one_line) = (0_usize, 0_u64, true);
    let mut index = blanks(bytes, start);
    loop {
        let target = target(bytes, index).filter(|_| bytes.get(index) == Some(&b'<'))?;
        let Some(end) = target.end else {
            return Some(RangeLine {
                entries: UNMEASURABLE,
                units: MAX_TARGET_UNITS,
                end: None,
                one_line: false,
            });
        };
        longest = longest.max(target.units);
        total = total.saturating_add(target.units as u64);
        one_line &= target.one_line;
        let next = blanks(bytes, end);
        match bytes.get(next)? {
            b']' => {
                return Some(RangeLine {
                    entries: span.saturating_mul(longest as u64).max(total),
                    units: longest,
                    end: Some(next + 1),
                    one_line,
                });
            }
            b'<' if next > end => index = next,
            _ => return None,
        }
    }
}

#[cfg(test)]
mod tests;
