//! Turning a worksheet's first row into object keys.

use std::borrow::Cow;
use std::collections::HashMap;

use anyhow::Result;
use calamine::Data;

use super::cells::cell_value;
use super::output_budget::OutputBudget;

/// Derive object keys from a header row.
///
/// Two rules differ from `csv_parse`, because spreadsheets are not CSVs: a
/// blank header cell becomes `column_{n}` rather than an empty-string key, and
/// duplicates gain `_2`, `_3` suffixes rather than overwriting. Repeated group
/// headers — two columns both labelled `Q1` — are normal, and last-wins would
/// drop real data without saying so.
pub(super) fn header_keys(
    header_row: &[Data],
    budget: &mut OutputBudget,
    sheet: &str,
) -> Result<Vec<String>> {
    // One map is both the taken-key set and the next suffix cursor per base.
    // A cursor only advances, so each occupied suffix is probed at most once
    // for that base instead of restarting at `_2` for every duplicate.
    let mut next_suffix: HashMap<String, usize> = HashMap::new();
    let mut keys = Vec::with_capacity(header_row.len());

    for (index, cell) in header_row.iter().enumerate() {
        let base = header_text(cell, index);
        keys.push(disambiguate(&base, &mut next_suffix, budget, sheet)?);
    }

    Ok(keys)
}

/// Borrow string cells so attacker-sized headers are not cloned before the
/// output budget approves their retained key copies. Scalar formatting is
/// bounded to a few dozen bytes.
fn header_text(cell: &Data, index: usize) -> Cow<'_, str> {
    let string = match cell {
        Data::String(value) | Data::DateTimeIso(value) | Data::DurationIso(value) => {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Cow::Borrowed(trimmed);
            }
            None
        }
        Data::Error(_) | Data::Empty => None,
        other => match cell_value(other) {
            serde_json::Value::Null => None,
            value => Some(value.to_string()),
        },
    };

    string.map_or_else(|| Cow::Owned(format!("column_{}", index + 1)), Cow::Owned)
}

/// Append `_2`, `_3`, … until the key is unused, charging before every key is
/// retained in the result and the collision map.
fn disambiguate(
    base: &str,
    next_suffix: &mut HashMap<String, usize>,
    budget: &mut OutputBudget,
    sheet: &str,
) -> Result<String> {
    if !next_suffix.contains_key(base) {
        budget.charge_structure((base.len() as u64).saturating_mul(2), sheet)?;
        let key = base.to_owned();
        next_suffix.insert(key.clone(), 2);
        return Ok(key);
    }

    let mut suffix = next_suffix
        .get(base)
        .copied()
        .ok_or_else(|| anyhow::anyhow!("extract_xlsx: internal header collision state was lost"))?;
    loop {
        let candidate_len = base
            .len()
            .saturating_add(1)
            .saturating_add(decimal_digits(suffix));
        // Charge the candidate buffer before formatting. A literal `a_2`
        // collision may discard this candidate, but cumulative work remains
        // bounded and the cursor never probes it again for base `a`.
        budget.charge_structure(candidate_len as u64, sheet)?;
        let candidate = format!("{base}_{suffix}");
        if !next_suffix.contains_key(&candidate) {
            budget.charge_structure(candidate_len as u64, sheet)?;
            next_suffix.insert(candidate.clone(), 2);
            let cursor = next_suffix.get_mut(base).ok_or_else(|| {
                anyhow::anyhow!("extract_xlsx: internal header collision state was lost")
            })?;
            *cursor = suffix
                .checked_add(1)
                .ok_or_else(|| anyhow::anyhow!("extract_xlsx: header suffix overflow"))?;
            return Ok(candidate);
        }
        suffix = suffix
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("extract_xlsx: header suffix overflow"))?;
    }
}

fn decimal_digits(value: usize) -> usize {
    value.ilog10() as usize + 1
}

#[cfg(test)]
mod tests {
    use super::super::output_budget::OutputBudget;
    use super::header_keys;
    use calamine::Data;

    fn text(values: &[&str]) -> Vec<Data> {
        values
            .iter()
            .map(|v| Data::String((*v).to_string()))
            .collect()
    }

    fn keys(values: &[Data]) -> Vec<String> {
        let mut budget = OutputBudget::new(64 * 1024 * 1024);
        header_keys(values, &mut budget, "Sheet").unwrap()
    }

    #[test]
    fn plain_headers_are_used_verbatim() {
        assert_eq!(keys(&text(&["name", "region"])), ["name", "region"]);
    }

    #[test]
    fn duplicate_headers_are_suffixed_rather_than_overwriting() {
        // Repeated group headers are normal in spreadsheets; csv_parse's
        // last-wins would silently drop a column of real data.
        assert_eq!(keys(&text(&["Q1", "Q1", "Q1"])), ["Q1", "Q1_2", "Q1_3"]);
    }

    #[test]
    fn blank_headers_become_positional_keys() {
        // Blank spacer columns are common, and an empty-string key is awkward
        // to reach from Lua.
        assert_eq!(
            keys(&[
                Data::String("name".into()),
                Data::Empty,
                Data::String("  ".into()),
            ]),
            ["name", "column_2", "column_3"]
        );
    }

    #[test]
    fn non_text_headers_are_stringified() {
        assert_eq!(
            keys(&[Data::Float(2026.0), Data::Bool(true)]),
            ["2026", "true"]
        );
    }

    #[test]
    fn a_suffixed_key_that_would_collide_keeps_advancing() {
        // "a", "a" -> "a", "a_2"; a literal "a_2" already present must not be
        // silently overwritten by the generated one.
        assert_eq!(keys(&text(&["a", "a_2", "a"])), ["a", "a_2", "a_3"]);
    }

    #[test]
    fn duplicate_generation_is_linear_at_the_xlsx_cell_ceiling() {
        let headers = vec![Data::String("Q".into()); 33_000];
        let generated = keys(&headers);

        assert_eq!(generated.len(), 33_000);
        assert_eq!(generated.first().map(String::as_str), Some("Q"));
        assert_eq!(generated.last().map(String::as_str), Some("Q_33000"));
    }

    #[test]
    fn a_near_limit_header_is_rejected_before_retained_key_clones() {
        let header = vec![Data::String("x".repeat(32))];
        let mut budget = OutputBudget::new(63);

        let error = header_keys(&header, &mut budget, "Wide")
            .unwrap_err()
            .to_string();
        assert!(error.contains("IRONFLOW_MAX_XLSX_OUTPUT_BYTES"), "{error}");
        assert!(error.contains("64"), "{error}");
    }
}
