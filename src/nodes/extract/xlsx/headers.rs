//! Turning a worksheet's first row into object keys.

use std::collections::HashSet;

use calamine::Data;

use super::cells::cell_value;

/// Derive object keys from a header row.
///
/// Two rules differ from `csv_parse`, because spreadsheets are not CSVs: a
/// blank header cell becomes `column_{n}` rather than an empty-string key, and
/// duplicates gain `_2`, `_3` suffixes rather than overwriting. Repeated group
/// headers — two columns both labelled `Q1` — are normal, and last-wins would
/// drop real data without saying so.
#[allow(dead_code)]
pub(super) fn header_keys(header_row: &[Data]) -> Vec<String> {
    let mut taken: HashSet<String> = HashSet::new();
    let mut keys = Vec::with_capacity(header_row.len());

    for (index, cell) in header_row.iter().enumerate() {
        let base = header_text(cell).unwrap_or_else(|| format!("column_{}", index + 1));
        keys.push(disambiguate(base, &mut taken));
    }

    keys
}

/// The header cell's text, or `None` when it is blank or whitespace-only.
fn header_text(cell: &Data) -> Option<String> {
    let text = match cell_value(cell) {
        serde_json::Value::Null => return None,
        serde_json::Value::String(value) => value,
        other => other.to_string(),
    };
    let trimmed = text.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

/// Append `_2`, `_3`, … until the key is unused.
fn disambiguate(base: String, taken: &mut HashSet<String>) -> String {
    if taken.insert(base.clone()) {
        return base;
    }
    let mut suffix = 2usize;
    loop {
        let candidate = format!("{base}_{suffix}");
        if taken.insert(candidate.clone()) {
            return candidate;
        }
        suffix += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::header_keys;
    use calamine::Data;

    fn text(values: &[&str]) -> Vec<Data> {
        values
            .iter()
            .map(|v| Data::String((*v).to_string()))
            .collect()
    }

    #[test]
    fn plain_headers_are_used_verbatim() {
        assert_eq!(header_keys(&text(&["name", "region"])), ["name", "region"]);
    }

    #[test]
    fn duplicate_headers_are_suffixed_rather_than_overwriting() {
        // Repeated group headers are normal in spreadsheets; csv_parse's
        // last-wins would silently drop a column of real data.
        assert_eq!(
            header_keys(&text(&["Q1", "Q1", "Q1"])),
            ["Q1", "Q1_2", "Q1_3"]
        );
    }

    #[test]
    fn blank_headers_become_positional_keys() {
        // Blank spacer columns are common, and an empty-string key is awkward
        // to reach from Lua.
        assert_eq!(
            header_keys(&[
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
            header_keys(&[Data::Float(2026.0), Data::Bool(true)]),
            ["2026", "true"]
        );
    }

    #[test]
    fn a_suffixed_key_that_would_collide_keeps_advancing() {
        // "a", "a" -> "a", "a_2"; a literal "a_2" already present must not be
        // silently overwritten by the generated one.
        assert_eq!(header_keys(&text(&["a", "a_2", "a"])), ["a", "a_2", "a_3"]);
    }
}
