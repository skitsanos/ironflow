//! One token is one field: the shape of the first four cron fields.
//!
//! The `cron` crate separates fields with optional whitespace, accepts an
//! optional trailing year, and skips whitespace around every number and name.
//! Handed `5*/2 * * * *` it reads minute `5` and hour `*/2`; handed
//! `/5 1* * * *` after the fixed seconds field it reads seconds `0/5`
//! (IF-206). This module accepts a token only when it is exactly one field, so
//! the crate can neither split a token nor join two. Values, bounds and month
//! names stay the crate's to check; the weekday field has its own module.

/// A field written before the weekday.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Field {
    Minute,
    Hour,
    DayOfMonth,
    Month,
}

/// The four leading fields in the order they are written.
const LEADING: [Field; 4] = [Field::Minute, Field::Hour, Field::DayOfMonth, Field::Month];

impl Field {
    fn label(self) -> &'static str {
        match self {
            Self::Minute => "minute",
            Self::Hour => "hour",
            Self::DayOfMonth => "day of month",
            Self::Month => "month",
        }
    }

    /// What one list item may be, worded for the error message.
    fn accepted(self) -> &'static str {
        match self {
            Self::Minute | Self::Hour => "*, a number or a range, optionally with /step",
            Self::DayOfMonth => "*, ?, a number or a range, optionally with /step",
            Self::Month => "*, a number or a range, optionally with /step, or a month name",
        }
    }
}

/// Check that each of the minute, hour, day-of-month and month tokens is one
/// field.
///
/// The error is one line that names the field and can be appended to the
/// caller's message.
pub(super) fn check(tokens: [&str; 4]) -> Result<(), String> {
    LEADING
        .into_iter()
        .zip(tokens)
        .try_for_each(|(field, token)| {
            token
                .split(',')
                .try_for_each(|item| check_item(field, item))
        })
}

fn check_item(field: Field, item: &str) -> Result<(), String> {
    if item.is_empty() {
        return Err(format!(
            "the {} field has an empty list item",
            field.label()
        ));
    }
    let (base, step) = match item.split_once('/') {
        Some((base, step)) => (base, Some(step)),
        None => (item, None),
    };
    let named = field == Field::Month;
    let one_value = match base.split_once('-') {
        Some((first, last)) => {
            (is_number(first) && is_number(last)) || (named && is_name(first) && is_name(last))
        }
        // The crate takes a step after `*`, `?`, a number or a range, not
        // after a single name.
        None => {
            base == "*"
                || (base == "?" && field == Field::DayOfMonth)
                || is_number(base)
                || (named && step.is_none() && is_name(base))
        }
    };
    if one_value && step.is_none_or(is_number) {
        return Ok(());
    }
    Err(format!(
        "{} '{item}' is not a single field; write {}, and separate fields with spaces",
        field.label(),
        field.accepted()
    ))
}

fn is_number(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit())
}

fn is_name(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_alphabetic())
}

#[cfg(test)]
mod tests {
    use super::check;

    fn check_one(position: usize, token: &str) -> Result<(), String> {
        let mut tokens = ["*"; 4];
        tokens[position] = token;
        check(tokens)
    }

    #[test]
    fn single_fields_are_accepted_in_every_position() {
        for token in [
            "*",
            "5",
            "007",
            "1-5",
            "*/2",
            "5/2",
            "1-5/2",
            "1,2",
            "1,3-5,*/2",
        ] {
            for position in 0..4 {
                assert_eq!(check_one(position, token), Ok(()), "{position}: {token}");
            }
        }
    }

    #[test]
    fn the_question_mark_belongs_to_the_day_of_month() {
        for token in ["?", "?/2", "1,?"] {
            assert_eq!(check_one(2, token), Ok(()), "{token}");
            for position in [0, 1, 3] {
                assert!(check_one(position, token).is_err(), "{position}: {token}");
            }
        }
    }

    #[test]
    fn names_belong_to_the_month() {
        for token in ["Jan", "jan-MAR", "Jan-Mar/2", "1,Feb", "Smarch"] {
            assert_eq!(check_one(3, token), Ok(()), "{token}");
            for position in 0..3 {
                assert!(check_one(position, token).is_err(), "{position}: {token}");
            }
        }
    }

    #[test]
    fn tokens_that_hold_two_fields_or_half_of_one_are_refused() {
        for token in [
            "5*/2",
            "1*",
            "*5",
            "**",
            "*?",
            "??",
            "Jan*",
            "Jan2030",
            "5Jan",
            "-5",
            "5-",
            "/5",
            "*/",
            "*/x",
            "*/-1",
            "1/2/3",
            "1-2-3",
            "Jan/2",
            "1-Jan",
            "Jan-3",
            "Jan-Feb-Mar",
            "+1",
            "1.5",
            "１",
        ] {
            for position in 0..4 {
                let error = check_one(position, token).unwrap_err();
                assert!(error.contains(&format!("'{token}'")), "{error}");
                assert!(!error.contains('\n'), "{error}");
            }
        }
        for token in ["", ",", "1,", ",1", "1,,2"] {
            let error = check_one(0, token).unwrap_err();
            assert!(
                error.contains("the minute field has an empty list item"),
                "{error}"
            );
        }
    }

    #[test]
    fn the_message_names_the_field_and_the_first_one_wins() {
        let labels = ["minute", "hour", "day of month", "month"];
        for (position, label) in labels.into_iter().enumerate() {
            let error = check_one(position, "1*").unwrap_err();
            assert!(error.starts_with(&format!("{label} '1*'")), "{error}");
        }
        let error = check(["*", "2*", "*", "1*"]).unwrap_err();
        assert!(error.starts_with("hour '2*'"), "{error}");
    }
}
