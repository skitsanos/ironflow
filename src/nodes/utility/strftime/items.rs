//! Reading a format string into items.
//!
//! The grammar is fixed: `%`, an optional padding modifier (`-`, `_`, `0`)
//! on a single numeric specifier, then the specifier. Anything else after a
//! `%` makes the whole string invalid, so a typo is reported instead of
//! being written out as text.

use super::Numeric::{Day, Hour, Hour12, Minute, Month, Second, Year, YearMod100};
use super::{Fixed, Item, Numeric, Pad};

const fn zero(numeric: Numeric) -> Item<'static> {
    Item::Numeric(numeric, Pad::Zero)
}

const fn space(numeric: Numeric) -> Item<'static> {
    Item::Numeric(numeric, Pad::Space)
}

const fn plain(numeric: Numeric) -> Item<'static> {
    Item::Numeric(numeric, Pad::None)
}

const MONTH_DAY_YEAR: &[Item<'static>] = &[
    zero(Month),
    Item::Literal("/"),
    zero(Day),
    Item::Literal("/"),
    zero(YearMod100),
];
const YEAR_MONTH_DAY: &[Item<'static>] = &[
    zero(Year),
    Item::Literal("-"),
    zero(Month),
    Item::Literal("-"),
    zero(Day),
];
const HOUR_MINUTE: &[Item<'static>] = &[zero(Hour), Item::Literal(":"), zero(Minute)];
const HOUR_MINUTE_SECOND: &[Item<'static>] = &[
    zero(Hour),
    Item::Literal(":"),
    zero(Minute),
    Item::Literal(":"),
    zero(Second),
];
const TWELVE_HOUR_CLOCK: &[Item<'static>] = &[
    zero(Hour12),
    Item::Literal(":"),
    zero(Minute),
    Item::Literal(":"),
    zero(Second),
    Item::Space(" "),
    Item::Fixed(Fixed::UpperAmPm),
];
const DAY_MONTH_YEAR: &[Item<'static>] = &[
    space(Day),
    Item::Literal("-"),
    Item::Fixed(Fixed::ShortMonthName),
    Item::Literal("-"),
    zero(Year),
];
const DATE_AND_TIME: &[Item<'static>] = &[
    Item::Fixed(Fixed::ShortWeekdayName),
    Item::Space(" "),
    Item::Fixed(Fixed::ShortMonthName),
    Item::Space(" "),
    space(Day),
    Item::Space(" "),
    zero(Hour),
    Item::Literal(":"),
    zero(Minute),
    Item::Literal(":"),
    zero(Second),
    Item::Space(" "),
    zero(Year),
];

/// What one specifier letter stands for.
enum Meaning {
    One(Item<'static>),
    Many(&'static [Item<'static>]),
}

/// The meaning of a specifier letter that takes no further characters.
fn letter(spec: char) -> Option<Meaning> {
    use Meaning::{Many, One};
    use Numeric::*;
    Some(match spec {
        'A' => One(Item::Fixed(Fixed::LongWeekdayName)),
        'B' => One(Item::Fixed(Fixed::LongMonthName)),
        'C' => One(zero(YearDiv100)),
        'D' | 'x' => Many(MONTH_DAY_YEAR),
        'F' => Many(YEAR_MONTH_DAY),
        'G' => One(zero(IsoYear)),
        'H' => One(zero(Hour)),
        'I' => One(zero(Hour12)),
        'M' => One(zero(Minute)),
        'P' => One(Item::Fixed(Fixed::LowerAmPm)),
        'R' => Many(HOUR_MINUTE),
        'S' => One(zero(Second)),
        'T' | 'X' => Many(HOUR_MINUTE_SECOND),
        'U' => One(zero(WeekFromSun)),
        'V' => One(zero(IsoWeek)),
        'W' => One(zero(WeekFromMon)),
        'Y' => One(zero(Year)),
        'Z' => One(Item::Fixed(Fixed::ZoneName)),
        'a' => One(Item::Fixed(Fixed::ShortWeekdayName)),
        'b' | 'h' => One(Item::Fixed(Fixed::ShortMonthName)),
        'c' => Many(DATE_AND_TIME),
        'd' => One(zero(Day)),
        'e' => One(space(Day)),
        'f' => One(zero(Nanosecond)),
        'g' => One(zero(IsoYearMod100)),
        'j' => One(zero(Ordinal)),
        'k' => One(space(Hour)),
        'l' => One(space(Hour12)),
        'm' => One(zero(Month)),
        'n' => One(Item::Space("\n")),
        'p' => One(Item::Fixed(Fixed::UpperAmPm)),
        'q' => One(plain(Quarter)),
        'r' => Many(TWELVE_HOUR_CLOCK),
        's' => One(plain(Timestamp)),
        't' => One(Item::Space("\t")),
        'u' => One(plain(WeekdayFromMon)),
        'v' => Many(DAY_MONTH_YEAR),
        'w' => One(plain(NumDaysFromSun)),
        'y' => One(zero(YearMod100)),
        'z' => One(Item::Fixed(Fixed::Offset { colons: 0 })),
        '+' => One(Item::Fixed(Fixed::Rfc3339)),
        '%' => One(Item::Literal("%")),
        _ => return None,
    })
}

/// Take the next character of `rest`, if any.
fn next_char(rest: &mut &str) -> Option<char> {
    let next = rest.chars().next()?;
    *rest = &rest[next.len_utf8()..];
    Some(next)
}

/// Read what follows a `%`. `None` is an unknown or incomplete specifier.
fn specifier(rest: &mut &str) -> Option<Meaning> {
    let first = next_char(rest)?;
    let pad = match first {
        '-' => Some(Pad::None),
        '0' => Some(Pad::Zero),
        '_' => Some(Pad::Space),
        _ => None,
    };
    if first == '#' {
        // Only `%#z` has an alternate form.
        return (next_char(rest)? == 'z')
            .then_some(Meaning::One(Item::Fixed(Fixed::OffsetPermissive)));
    }
    let Some(pad) = pad else {
        return unpadded(first, rest);
    };
    // A padding modifier applies to a single numeric specifier only.
    let spec = next_char(rest)?;
    match unpadded(spec, rest)? {
        Meaning::One(Item::Numeric(numeric, _)) => Some(Meaning::One(Item::Numeric(numeric, pad))),
        _ => None,
    }
}

/// A specifier without a padding modifier, starting at `spec`.
fn unpadded(spec: char, rest: &mut &str) -> Option<Meaning> {
    let fixed = match spec {
        ':' => {
            let colons = if let Some(tail) = rest.strip_prefix("::z") {
                *rest = tail;
                3
            } else if let Some(tail) = rest.strip_prefix(":z") {
                *rest = tail;
                2
            } else {
                *rest = rest.strip_prefix('z')?;
                1
            };
            Fixed::Offset { colons }
        }
        '.' => match next_char(rest)? {
            'f' => Fixed::Fraction,
            digit @ ('3' | '6' | '9') => fraction_digits(digit, true, rest)?,
            _ => return None,
        },
        digit @ ('3' | '6' | '9') => fraction_digits(digit, false, rest)?,
        _ => return letter(spec),
    };
    Some(Meaning::One(Item::Fixed(fixed)))
}

fn fraction_digits(digit: char, dot: bool, rest: &mut &str) -> Option<Fixed> {
    (next_char(rest)? == 'f').then_some(Fixed::FractionDigits {
        digits: digit.to_digit(10)? as usize,
        dot,
    })
}

/// Split a format string into items, or `None` when it has an unknown or
/// incomplete specifier.
pub(in crate::nodes::utility) fn items(format: &str) -> Option<Vec<Item<'_>>> {
    let mut items = Vec::new();
    let mut rest = format;
    while let Some(first) = rest.chars().next() {
        if first == '%' {
            rest = &rest[1..];
            match specifier(&mut rest)? {
                Meaning::One(item) => items.push(item),
                Meaning::Many(queue) => items.extend_from_slice(queue),
            }
        } else if first.is_whitespace() {
            let end = rest
                .find(|c: char| !c.is_whitespace())
                .unwrap_or(rest.len());
            items.push(Item::Space(&rest[..end]));
            rest = &rest[end..];
        } else {
            let end = rest
                .find(|c: char| c.is_whitespace() || c == '%')
                .unwrap_or(rest.len());
            items.push(Item::Literal(&rest[..end]));
            rest = &rest[end..];
        }
    }
    Some(items)
}
