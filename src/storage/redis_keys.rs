use std::borrow::Cow;

/// Preserve existing UUID-like key segments while escaping values that can
/// alias Redis bookkeeping keys. The `~` namespace is reserved for hex-encoded
/// UTF-8 bytes, making the mapping injective.
pub(crate) fn run_segment(run_id: &str) -> Cow<'_, str> {
    if is_legacy_safe_run_id(run_id) {
        Cow::Borrowed(run_id)
    } else {
        Cow::Owned(format!("~{}", hex::encode(run_id.as_bytes())))
    }
}

pub(crate) fn is_legacy_safe_run_id(run_id: &str) -> bool {
    !run_id.is_empty()
        && run_id != "index"
        && run_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

#[cfg(test)]
mod tests {
    use super::run_segment;

    #[test]
    fn preserves_uuid_keys_and_escapes_aliases_injectively() {
        assert_eq!(run_segment("run-123"), "run-123");
        assert_ne!(run_segment("index"), "index");
        assert_ne!(run_segment("run:index"), "run:index");
        assert_ne!(run_segment("~72756e"), run_segment("run"));
        assert_ne!(run_segment(""), "");
    }
}
