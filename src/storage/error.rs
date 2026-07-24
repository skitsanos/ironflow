use std::fmt;

use crate::util::sensitive_url::redact_sensitive_text;

/// Stable error categories shared by state and event stores.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum StorageErrorKind {
    InvalidInput,
    NotFound,
    Backend,
    Corruption,
    Conflict,
}

/// A storage failure whose diagnostic is safe to display or log.
///
/// Driver errors are converted to sanitized text instead of retained as an
/// error source. This prevents callers that print a full source chain from
/// accidentally recovering credentials embedded in a connection URL.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct StorageError {
    kind: StorageErrorKind,
    diagnostic: String,
}

impl StorageError {
    pub fn invalid_input(diagnostic: impl fmt::Display) -> Self {
        Self::new(StorageErrorKind::InvalidInput, diagnostic)
    }

    pub fn not_found(diagnostic: impl fmt::Display) -> Self {
        Self::new(StorageErrorKind::NotFound, diagnostic)
    }

    pub fn backend(operation: impl fmt::Display, cause: impl fmt::Display) -> Self {
        Self::with_cause(StorageErrorKind::Backend, operation, cause)
    }

    pub fn corruption(operation: impl fmt::Display, cause: impl fmt::Display) -> Self {
        Self::with_cause(StorageErrorKind::Corruption, operation, cause)
    }

    pub fn conflict(diagnostic: impl fmt::Display) -> Self {
        Self::new(StorageErrorKind::Conflict, diagnostic)
    }

    pub const fn kind(&self) -> StorageErrorKind {
        self.kind
    }

    pub const fn is_not_found(&self) -> bool {
        matches!(self.kind, StorageErrorKind::NotFound)
    }

    pub fn diagnostic(&self) -> &str {
        &self.diagnostic
    }

    fn new(kind: StorageErrorKind, diagnostic: impl fmt::Display) -> Self {
        Self {
            kind,
            diagnostic: redact_sensitive_text(&diagnostic.to_string()),
        }
    }

    fn with_cause(
        kind: StorageErrorKind,
        operation: impl fmt::Display,
        cause: impl fmt::Display,
    ) -> Self {
        Self::new(kind, format_args!("{operation}: {cause}"))
    }
}

impl fmt::Display for StorageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.diagnostic)
    }
}

impl std::error::Error for StorageError {}

pub type StorageResult<T> = Result<T, StorageError>;

#[cfg(test)]
mod tests {
    use super::{StorageError, StorageErrorKind};

    #[test]
    fn diagnostics_are_sanitized_before_storage() {
        let error = StorageError::backend(
            "connect Redis",
            "redis://operator:very-secret@example.test/ failed",
        );

        assert_eq!(error.kind(), StorageErrorKind::Backend);
        assert!(!error.to_string().contains("very-secret"));
        assert!(!format!("{error:?}").contains("very-secret"));
    }

    #[test]
    fn invalid_input_has_a_distinct_public_category() {
        let error = StorageError::invalid_input("run ID has an invalid shape");

        assert_eq!(error.kind(), StorageErrorKind::InvalidInput);
        assert_eq!(error.to_string(), "run ID has an invalid shape");
    }
}
