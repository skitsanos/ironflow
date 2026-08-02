//! Bounded rendering of attacker-controlled workbook labels in errors.

use std::borrow::Cow;
use std::fmt::Write;

const MAX_LABEL_BYTES: usize = 160;
const MAX_LISTED_SHEETS: usize = 4;

pub(super) fn label(value: &str) -> Cow<'_, str> {
    if value.len() <= MAX_LABEL_BYTES {
        return Cow::Borrowed(value);
    }
    let end = floor_char_boundary(value, MAX_LABEL_BYTES);
    Cow::Owned(format!("{}… ({} bytes)", &value[..end], value.len()))
}

pub(super) fn available<'a>(names: impl Iterator<Item = &'a str>, total: usize) -> String {
    if total == 0 {
        return "no sheets".to_string();
    }

    let shown = total.min(MAX_LISTED_SHEETS);
    let mut output = String::with_capacity(shown.saturating_mul(MAX_LABEL_BYTES));
    for (index, name) in names.take(shown).enumerate() {
        if index > 0 {
            output.push_str(", ");
        }
        let _ = write!(output, "'{}'", label(name));
    }
    if total > shown {
        let _ = write!(output, ", … ({} more)", total - shown);
    }
    output
}

fn floor_char_boundary(value: &str, mut index: usize) -> usize {
    index = index.min(value.len());
    while !value.is_char_boundary(index) {
        index -= 1;
    }
    index
}

#[cfg(test)]
mod tests {
    use super::{available, label};

    #[test]
    fn labels_are_utf8_safe_and_bounded() {
        let value = "🧪".repeat(1_000);
        let rendered = label(&value);

        assert!(rendered.len() < 200);
        assert!(rendered.contains("4000 bytes"));
    }

    #[test]
    fn available_sheet_lists_are_bounded_by_count_and_label_size() {
        let names = (0..100)
            .map(|index| format!("{index}-{}", "x".repeat(10_000)))
            .collect::<Vec<_>>();
        let rendered = available(names.iter().map(String::as_str), names.len());

        assert!(rendered.len() < 800, "{} bytes", rendered.len());
        assert!(rendered.contains("96 more"), "{rendered}");
    }
}
