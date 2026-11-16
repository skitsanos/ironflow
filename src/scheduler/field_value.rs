//! Values, ranges and steps of the first four cron fields (IF-217).
//!
//! `field_shape` has already established that each token is one field. The
//! `cron` crate would refuse a value outside the field, a backwards range or
//! a bad step, but its error cannot be shown: it describes the six-field
//! expression the scheduler builds, not the five fields the author wrote.
//! This module applies the same bounds first, so the refusal names the field
//! and quotes the list item that is wrong.

use super::field_shape::Field;

impl Field {
    /// Lowest and highest value the field holds.
    fn bounds(self) -> (u32, u32) {
        match self {
            Self::Minute => (0, 59),
            Self::Hour => (0, 23),
            Self::DayOfMonth => (1, 31),
            Self::Month => (1, 12),
        }
    }
}

/// Check one well-shaped list item of `field`.
///
/// The error is one line that names the field and quotes the item.
pub(super) fn check_item(field: Field, item: &str) -> Result<(), String> {
    let label = field.label();
    let (low, high) = field.bounds();
    let (base, step) = match item.split_once('/') {
        Some((base, step)) => (base, Some(step)),
        None => (item, None),
    };

    match base.split_once('-') {
        Some((first, last)) => {
            let first = value(field, item, first)?;
            let last = value(field, item, last)?;
            if first > last {
                return Err(format!(
                    "{label} '{item}' is a range that runs backwards; \
                     write the lower value first or list the values"
                ));
            }
        }
        None if matches!(base, "*" | "?") => {}
        None => {
            value(field, item, base)?;
        }
    }

    // The crate accepts a step from 1 to the field's highest value.
    let step_fits = step.is_none_or(|step| {
        step.parse::<u32>()
            .is_ok_and(|step| (1..=high).contains(&step))
    });
    if step_fits {
        Ok(())
    } else {
        Err(format!(
            "{label} '{item}' has a step outside 1 to {high} \
             (the field holds {low} to {high})"
        ))
    }
}

/// The number one part of `item` stands for, checked against the field.
fn value(field: Field, item: &str, part: &str) -> Result<u32, String> {
    let label = field.label();
    let (low, high) = field.bounds();
    let named = part.bytes().all(|byte| byte.is_ascii_alphabetic());
    let found = if named {
        month_number(part)
    } else {
        part.parse::<u32>()
            .ok()
            .filter(|number| (low..=high).contains(number))
    };
    found.ok_or_else(|| {
        let problem = if named {
            "not a month name".to_string()
        } else {
            format!("outside {low} to {high}")
        };
        if part == item {
            format!("{label} '{item}' is {problem}")
        } else {
            format!("{label} '{item}' holds '{part}', which is {problem}")
        }
    })
}

/// Number of a month name the crate accepts.
fn month_number(name: &str) -> Option<u32> {
    const MONTHS: [(&str, &str); 12] = [
        ("jan", "january"),
        ("feb", "february"),
        ("mar", "march"),
        ("apr", "april"),
        ("may", "may"),
        ("jun", "june"),
        ("jul", "july"),
        ("aug", "august"),
        ("sep", "september"),
        ("oct", "october"),
        ("nov", "november"),
        ("dec", "december"),
    ];
    let name = name.to_ascii_lowercase();
    (1..)
        .zip(MONTHS)
        .find_map(|(number, (short, long))| (name == short || name == long).then_some(number))
}

#[cfg(test)]
mod tests {
    use super::{Field, check_item};

    #[test]
    fn every_bound_of_every_field_is_enforced() {
        for (field, low, high) in [
            (Field::Minute, 0, 59),
            (Field::Hour, 0, 23),
            (Field::DayOfMonth, 1, 31),
            (Field::Month, 1, 12),
        ] {
            for inside in [low, high] {
                assert_eq!(check_item(field, &inside.to_string()), Ok(()));
            }
            let above = (high + 1).to_string();
            let error = check_item(field, &above).unwrap_err();
            assert!(
                error.contains(&format!("'{above}' is outside {low} to {high}")),
                "{error}"
            );
            if low > 0 {
                assert!(check_item(field, "0").is_err());
            }
            assert_eq!(check_item(field, &format!("*/{high}")), Ok(()));
            for step in [0, high + 1] {
                let error = check_item(field, &format!("*/{step}")).unwrap_err();
                assert!(
                    error.contains(&format!("step outside 1 to {high}")),
                    "{error}"
                );
            }
        }
    }

    #[test]
    fn month_names_are_checked_like_month_numbers() {
        for item in ["Jan", "DECEMBER", "jan-mar", "Feb-Jun/2", "May-May"] {
            assert_eq!(check_item(Field::Month, item), Ok(()), "{item}");
        }
        for (item, problem) in [
            ("Smarch", "'Smarch' is not a month name"),
            ("Jan-Smarch", "holds 'Smarch', which is not a month name"),
            ("Mar-Jan", "runs backwards"),
        ] {
            let error = check_item(Field::Month, item).unwrap_err();
            assert!(
                error.starts_with("month '") && error.contains(problem),
                "{error}"
            );
        }
    }
}
