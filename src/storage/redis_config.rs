use anyhow::{Result, ensure};

use crate::storage::StorageError;

/// Largest TTL that remains safe across IronFlow's Redis Lua scripts.
///
/// Keeping the configured seconds at or below this value guarantees that its
/// millisecond representation stays below `10^14`
/// (`99_999_999_999_000`). That is exactly representable by Redis's embedded
/// Lua runtime and can be formatted as a decimal `PEXPIRE` argument, while still
/// allowing retention periods of more than three millennia.
pub(crate) const MAX_REDIS_TTL_SECONDS: u64 = 99_999_999_999;

pub(crate) fn validate_redis_ttl(ttl: Option<u64>) -> Result<Option<i64>> {
    ttl.map(|value| {
        ensure!(value > 0, "Redis TTL must be greater than zero");
        ensure!(
            value <= MAX_REDIS_TTL_SECONDS,
            "Redis TTL must not exceed {MAX_REDIS_TTL_SECONDS} seconds"
        );
        Ok(value as i64)
    })
    .transpose()
}

pub(crate) fn map_redis_error(
    operation: impl std::fmt::Display,
    error: redis::RedisError,
) -> StorageError {
    let code = error.code().unwrap_or_default();
    let detail = error.detail().unwrap_or_default();
    let contains_marker = |marker: &str| code == marker || detail.contains(marker);

    if contains_marker("IRONFLOW_RUN_NOT_FOUND")
        || contains_marker("IRONFLOW_EVENT_CURSOR_NOT_FOUND")
    {
        StorageError::not_found(operation)
    } else if contains_marker("IRONFLOW_EVENT_ID_CONFLICT")
        || contains_marker("IRONFLOW_EVENT_STREAM_DELETED")
        || contains_marker("IRONFLOW_RUN_RECREATED")
    {
        StorageError::conflict(operation)
    } else if code == "WRONGTYPE" || code.starts_with("IRONFLOW_") || detail.contains("IRONFLOW_") {
        StorageError::corruption(operation, error)
    } else {
        StorageError::backend(operation, error)
    }
}

#[cfg(test)]
mod tests {
    use super::{MAX_REDIS_TTL_SECONDS, validate_redis_ttl};

    #[test]
    fn validates_the_lua_safe_ttl_range() {
        assert_eq!(validate_redis_ttl(None).unwrap(), None);
        assert_eq!(validate_redis_ttl(Some(1)).unwrap(), Some(1));
        assert_eq!(
            validate_redis_ttl(Some(MAX_REDIS_TTL_SECONDS)).unwrap(),
            Some(MAX_REDIS_TTL_SECONDS as i64)
        );
        assert!(validate_redis_ttl(Some(0)).is_err());
        assert!(validate_redis_ttl(Some(MAX_REDIS_TTL_SECONDS + 1)).is_err());
        assert!(validate_redis_ttl(Some(u64::MAX)).is_err());
    }
}
