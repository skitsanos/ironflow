use std::fmt;

pub const MAX_RUN_ID_BYTES: usize = 128;

/// Why a run ID is outside the canonical public/JSON-safe format.
///
/// Display messages deliberately never include the rejected input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunIdError {
    Empty,
    TooLong,
    InvalidCharacter,
    InvalidBoundary,
}

impl fmt::Display for RunIdError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Empty => "run ID must not be empty",
            Self::TooLong => "run ID must not exceed 128 bytes",
            Self::InvalidCharacter => "run ID may contain only ASCII letters, digits, '-' and '_'",
            Self::InvalidBoundary => "run ID must start and end with an ASCII letter or digit",
        })
    }
}

impl std::error::Error for RunIdError {}

/// Validate the canonical run ID accepted by public APIs and filesystem stores.
///
/// Validation is byte-exact: input is never trimmed, case-folded, or otherwise
/// normalized.
pub fn validate_run_id(run_id: &str) -> Result<(), RunIdError> {
    let bytes = run_id.as_bytes();
    if bytes.is_empty() {
        return Err(RunIdError::Empty);
    }
    if bytes.len() > MAX_RUN_ID_BYTES {
        return Err(RunIdError::TooLong);
    }
    if !bytes
        .iter()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(RunIdError::InvalidCharacter);
    }
    if !bytes.first().is_some_and(u8::is_ascii_alphanumeric)
        || !bytes.last().is_some_and(u8::is_ascii_alphanumeric)
    {
        return Err(RunIdError::InvalidBoundary);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_the_canonical_ascii_shape() {
        for valid in ["a", "A9", "run-123", "run_123", "a-_-Z"] {
            assert_eq!(validate_run_id(valid), Ok(()), "rejected {valid:?}");
        }

        for invalid in [
            "", "-run", "run-", "_run", "run_", "run.id", "run/id", "run id", "rún",
        ] {
            assert!(validate_run_id(invalid).is_err(), "accepted {invalid:?}");
        }
    }

    #[test]
    fn applies_the_limit_to_exact_input_bytes() {
        assert_eq!(validate_run_id(&"a".repeat(MAX_RUN_ID_BYTES)), Ok(()));
        assert_eq!(
            validate_run_id(&"a".repeat(MAX_RUN_ID_BYTES + 1)),
            Err(RunIdError::TooLong)
        );
        assert_eq!(
            validate_run_id(" valid-id "),
            Err(RunIdError::InvalidCharacter)
        );
    }

    #[test]
    fn errors_do_not_echo_rejected_input() {
        let rejected = "secret-traversal/../../outside";
        let message = validate_run_id(rejected).unwrap_err().to_string();
        assert!(!message.contains(rejected));
        assert!(!message.contains("secret-traversal"));
    }
}
