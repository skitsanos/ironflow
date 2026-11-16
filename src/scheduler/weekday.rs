//! Standard crontab weekday numbering for the fifth cron field.
//!
//! Authors write `0` or `7` for Sunday and `1` to `6` for Monday to Saturday.
//! The `cron` crate numbers the week `1` to `7` starting on Sunday, so a
//! numeric weekday handed to it unchanged fires one day early and `0` is
//! refused (IF-180). This module validates the field and rewrites every
//! number, range and step into the crate's numbering. Names, `*` and `?` mean
//! the same days in both numberings and pass through, except a named range
//! that crosses the week boundary (`Sat-Sun`, `Fri-Mon`): the crate reads it
//! as running backwards, so it is expanded here (IF-217).

/// Sunday written as the end of the week; also the largest weekday number.
const SUNDAY_AS_SEVEN: u8 = 7;

const NUMBERING: &str = "0 and 7 are Sunday, 1 to 6 are Monday to Saturday";

/// Rewrite one weekday field from standard numbering into the `cron` crate's.
///
/// The error is one line that names the weekday field and can be appended to
/// the caller's message.
pub(super) fn normalize(field: &str) -> Result<String, String> {
    let items = field
        .split(',')
        .map(normalize_item)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(items.join(","))
}

fn normalize_item(item: &str) -> Result<String, String> {
    if item.is_empty() {
        return Err("the weekday field has an empty list item".to_string());
    }
    let (base, step) = match item.split_once('/') {
        Some((base, step)) => (base, Some(parse_step(item, step)?)),
        None => (item, None),
    };
    if let Some((first, last)) = wrapping_named_range(base) {
        return Ok(crate_days(first, last, step));
    }
    if is_named(base) {
        check_names(item, base, step.is_some())?;
        // The crate resolves names itself.
        return Ok(item.to_string());
    }
    if matches!(base, "*" | "?") {
        // A step over `*` starts on Sunday in both numberings.
        return Ok(item.to_string());
    }

    let (first, last) = match base.split_once('-') {
        Some((first, last)) => (parse_day(item, first)?, parse_day(item, last)?),
        None => {
            let day = parse_day(item, base)?;
            // `N/S` starts at `N` and runs to the end of the week.
            (day, if step.is_some() { SUNDAY_AS_SEVEN } else { day })
        }
    };
    if first > last {
        return Err(format!(
            "weekday range '{item}' runs backwards; list the days instead ({NUMBERING})"
        ));
    }

    Ok(crate_days(first, last, step))
}

/// The days `first..=last` in standard numbering, every `step`th, written in
/// the crate's numbering. `last` may run past 7 for a range that wraps.
fn crate_days(first: u8, last: u8, step: Option<usize>) -> String {
    let days = (first..=last)
        .step_by(step.unwrap_or(1))
        .map(|day| (day % 7 + 1).to_string())
        .collect::<Vec<_>>();
    days.join(",")
}

/// A range of two known day names whose first day is later in the week than
/// its last, as standard day numbers with the last moved into the next week:
/// `Fri-Mon` is 5 to 8. A range that does not wrap, or that holds an unknown
/// name, is `None` and stays the crate's to read.
fn wrapping_named_range(base: &str) -> Option<(u8, u8)> {
    let (first, last) = base.split_once('-')?;
    let (first, last) = (day_number(first)?, day_number(last)?);
    (first > last).then_some((first, last + SUNDAY_AS_SEVEN))
}

/// Standard number (Sunday is 0) of a day name the crate accepts.
fn day_number(name: &str) -> Option<u8> {
    Some(match name.to_ascii_lowercase().as_str() {
        "sun" | "sunday" => 0,
        "mon" | "monday" => 1,
        "tue" | "tues" | "tuesday" => 2,
        "wed" | "wednesday" => 3,
        "thu" | "thurs" | "thursday" => 4,
        "fri" | "friday" => 5,
        "sat" | "saturday" => 6,
        _ => return None,
    })
}

/// A day name or a range of two names, which the crate resolves.
fn is_named(base: &str) -> bool {
    let name = |part: &str| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_alphabetic());
    match base.split_once('-') {
        Some((first, last)) => name(first) && name(last),
        None => name(base),
    }
}

/// Refuse what the crate would refuse in a named item, naming the field: an
/// unknown day name, and a step after a single name.
fn check_names(item: &str, base: &str, stepped: bool) -> Result<(), String> {
    let (first, last) = match base.split_once('-') {
        Some((first, last)) => (first, Some(last)),
        None => (base, None),
    };
    for name in std::iter::once(first).chain(last) {
        if day_number(name).is_none() {
            return Err(if name == item {
                format!("weekday '{item}' is not a day name ({NUMBERING})")
            } else {
                format!("weekday '{item}' holds '{name}', which is not a day name")
            });
        }
    }
    if stepped && last.is_none() {
        return Err(format!(
            "weekday step in '{item}' cannot follow a single day name; \
             write a range such as Mon-Sat/2"
        ));
    }
    Ok(())
}

fn parse_day(item: &str, text: &str) -> Result<u8, String> {
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(format!(
            "weekday '{item}' is not a day number from 0 to 7, a day name, \
             or a range or step of either ({NUMBERING})"
        ));
    }
    text.parse::<u8>()
        .ok()
        .filter(|day| *day <= SUNDAY_AS_SEVEN)
        .ok_or_else(|| format!("weekday {text} is outside 0 to 7 ({NUMBERING})"))
}

fn parse_step(item: &str, text: &str) -> Result<usize, String> {
    let digits = !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit());
    digits
        .then(|| text.parse::<usize>().ok())
        .flatten()
        .filter(|step| (1..=usize::from(SUNDAY_AS_SEVEN)).contains(step))
        .ok_or_else(|| format!("weekday step in '{item}' must be a number from 1 to 7"))
}

#[cfg(test)]
mod tests {
    use super::normalize;

    #[test]
    fn numbers_are_rewritten_into_the_sunday_first_numbering() {
        for (written, rewritten) in [
            ("0", "1"),
            ("7", "1"),
            ("1", "2"),
            ("6", "7"),
            ("1-5", "2,3,4,5,6"),
            ("5-7", "6,7,1"),
            ("6,0", "7,1"),
            ("1-5/2", "2,4,6"),
            ("0-6/3", "1,4,7"),
            ("1/2", "2,4,6,1"),
            ("7/2", "1"),
            ("007", "1"),
        ] {
            assert_eq!(normalize(written).as_deref(), Ok(rewritten), "{written}");
        }
    }

    #[test]
    fn names_wildcards_and_their_steps_pass_through() {
        for field in ["*", "?", "*/2", "?/3", "Mon", "mon-FRI", "Mon-Fri/2"] {
            assert_eq!(normalize(field).as_deref(), Ok(field));
        }
        assert_eq!(normalize("Mon,3,*/2").as_deref(), Ok("Mon,4,*/2"));
    }

    #[test]
    fn named_ranges_across_the_week_boundary_are_expanded() {
        for (written, rewritten) in [
            ("Sat-Sun", "7,1"),
            ("Fri-Mon", "6,7,1,2"),
            ("saturday-TUES", "7,1,2,3"),
            ("Mon-Sun", "2,3,4,5,6,7,1"),
            ("Fri-Mon/2", "6,1"),
            ("Sat-Sun/7", "7"),
        ] {
            assert_eq!(normalize(written).as_deref(), Ok(rewritten), "{written}");
        }
        // The numeric form of the same days.
        assert_eq!(normalize("Sat-Sun"), normalize("6-7"));
        // An unknown name is refused, not read as a wrap.
        let error = normalize("Sat-Funday").unwrap_err();
        assert!(
            error.contains("'Funday', which is not a day name"),
            "{error}"
        );
    }

    #[test]
    fn numbers_outside_the_week_and_malformed_items_are_refused() {
        for field in [
            "8",
            "1-8",
            "256",
            "99999999999999999999",
            "6-1",
            "1-5/0",
            "1-5/8",
            "*/0",
            "*/8",
            "*/x",
            "1/",
            "",
            "1,",
            ",1",
            "1-",
            "-1",
            "+1",
            "1*",
            "*5",
            "Mon2030",
            "Mon-5",
            "1-Fri",
            "Mon-Fri-Sun",
            "Funday",
            "Mon/2",
            "1-2-3",
            "1/2/3",
            "１",
        ] {
            let error = normalize(field).unwrap_err();
            assert!(error.contains("weekday"), "{field}: {error}");
            assert!(!error.contains('\n'), "{field}: {error}");
        }
    }
}
