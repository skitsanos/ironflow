use std::fmt;

use crate::storage::PageSize;

pub const MAX_LIST_RECORDS_ENV: &str = "IRONFLOW_MAX_LIST_RECORDS";
pub const DEFAULT_MAX_LIST_RECORDS: usize = 100;
pub const DEFAULT_API_LIST_RECORDS: usize = 50;

#[derive(Clone, Copy, Debug)]
pub struct ListingPolicy {
    max_records: PageSize,
}

impl ListingPolicy {
    pub fn from_env() -> Result<Self, ListingPolicyError> {
        Self::from_value(std::env::var(MAX_LIST_RECORDS_ENV).ok().as_deref())
    }

    pub fn from_value(value: Option<&str>) -> Result<Self, ListingPolicyError> {
        let max_records = match value {
            None => DEFAULT_MAX_LIST_RECORDS,
            Some(value) => value
                .trim()
                .parse::<usize>()
                .ok()
                .filter(|value| *value > 0)
                .ok_or(ListingPolicyError::InvalidConfiguration)?,
        };
        Ok(Self {
            max_records: PageSize::new(max_records)
                .map_err(|_| ListingPolicyError::InvalidConfiguration)?,
        })
    }

    pub fn api_page_size(self, requested: Option<usize>) -> Result<PageSize, ListingPolicyError> {
        self.page_size(requested, DEFAULT_API_LIST_RECORDS.min(self.max_records()))
    }

    pub fn cli_page_size(self, requested: Option<usize>) -> Result<PageSize, ListingPolicyError> {
        self.page_size(requested, self.max_records())
    }

    pub const fn max_records(self) -> usize {
        self.max_records.get()
    }

    fn page_size(
        self,
        requested: Option<usize>,
        default: usize,
    ) -> Result<PageSize, ListingPolicyError> {
        let value = requested.unwrap_or(default);
        if value == 0 {
            return Err(ListingPolicyError::ZeroPageSize);
        }
        if value > self.max_records() {
            return Err(ListingPolicyError::PageSizeExceeded {
                requested: value,
                maximum: self.max_records(),
            });
        }
        PageSize::new(value).map_err(|_| ListingPolicyError::ZeroPageSize)
    }
}

impl Default for ListingPolicy {
    fn default() -> Self {
        Self::from_value(None).expect("the default listing policy is valid")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListingPolicyError {
    InvalidConfiguration,
    ZeroPageSize,
    PageSizeExceeded { requested: usize, maximum: usize },
}

impl fmt::Display for ListingPolicyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfiguration => write!(
                formatter,
                "{MAX_LIST_RECORDS_ENV} must be a positive integer"
            ),
            Self::ZeroPageSize => formatter.write_str("list limit must be greater than zero"),
            Self::PageSizeExceeded { requested, maximum } => write!(
                formatter,
                "requested list limit {requested} exceeds {MAX_LIST_RECORDS_ENV} ({maximum})"
            ),
        }
    }
}

impl std::error::Error for ListingPolicyError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_and_explicit_caps_are_positive_and_enforced() {
        let default = ListingPolicy::from_value(None).unwrap();
        assert_eq!(default.max_records(), 100);
        assert_eq!(default.api_page_size(None).unwrap().get(), 50);
        assert_eq!(default.cli_page_size(None).unwrap().get(), 100);

        let small = ListingPolicy::from_value(Some(" 3 ")).unwrap();
        assert_eq!(small.api_page_size(None).unwrap().get(), 3);
        assert_eq!(
            small.api_page_size(Some(4)).unwrap_err(),
            ListingPolicyError::PageSizeExceeded {
                requested: 4,
                maximum: 3,
            }
        );
    }

    #[test]
    fn invalid_configuration_and_zero_request_never_mean_unlimited() {
        for value in [Some(""), Some("0"), Some("-1"), Some("many")] {
            assert_eq!(
                ListingPolicy::from_value(value).unwrap_err(),
                ListingPolicyError::InvalidConfiguration
            );
        }
        let maximum = usize::MAX.to_string();
        assert_eq!(
            ListingPolicy::from_value(Some(&maximum)).unwrap_err(),
            ListingPolicyError::InvalidConfiguration
        );
        assert_eq!(
            ListingPolicy::default().api_page_size(Some(0)).unwrap_err(),
            ListingPolicyError::ZeroPageSize
        );
    }
}
