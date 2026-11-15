//! A byte ceiling on one S3 list response (IF-215).
//!
//! The SDK reads a list response whole before it parses it, so the entry
//! count of a page can only be checked after the page is in memory. An
//! endpoint could therefore send one very large page. The ceiling is applied
//! to the response before the SDK reads it: a declared length above it is
//! refused at once, and a body without a length is cut off while streaming.

use std::error::Error;
use std::fmt;

use aws_sdk_s3::config::interceptors::BeforeDeserializationInterceptorContextMut;
use aws_sdk_s3::config::{ConfigBag, Intercept, RuntimeComponents};
use aws_sdk_s3::error::BoxError;
use aws_smithy_types::body::SdkBody;
use http_body_util::{LengthLimitError, Limited};

/// Refuses a response body larger than `limit` bytes.
#[derive(Debug)]
pub(super) struct PageByteLimit {
    limit: u64,
}

impl PageByteLimit {
    /// The ceiling from `IRONFLOW_MAX_S3_LIST_PAGE_BYTES`.
    pub(super) fn current() -> Self {
        Self {
            limit: crate::util::limits::max_s3_list_page_bytes(),
        }
    }

    pub(super) fn limit(&self) -> u64 {
        self.limit
    }
}

/// A response that declared a length above the ceiling.
#[derive(Debug)]
struct DeclaredTooLarge;

impl fmt::Display for DeclaredTooLarge {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("declared response length exceeds the list page limit")
    }
}

impl Error for DeclaredTooLarge {}

impl Intercept for PageByteLimit {
    fn name(&self) -> &'static str {
        "IronFlowListPageByteLimit"
    }

    fn modify_before_deserialization(
        &self,
        context: &mut BeforeDeserializationInterceptorContextMut<'_>,
        _runtime_components: &RuntimeComponents,
        _cfg: &mut ConfigBag,
    ) -> Result<(), BoxError> {
        let response = context.response_mut();
        let declared = response
            .headers()
            .get("content-length")
            .and_then(|length| length.parse::<u64>().ok());
        if declared.is_some_and(|length| length > self.limit) {
            return Err(Box::new(DeclaredTooLarge));
        }
        let body = response.take_body();
        let limit = usize::try_from(self.limit).unwrap_or(usize::MAX);
        *response.body_mut() = SdkBody::from_body_1_x(Limited::new(body, limit));
        Ok(())
    }
}

/// The node error for a failure this ceiling caused, or `None` when `error`
/// has another cause.
pub(super) fn refusal(
    node: &str,
    error: &(dyn Error + 'static),
    limit: u64,
) -> Option<anyhow::Error> {
    let mut cause = Some(error);
    while let Some(error) = cause {
        if error.is::<DeclaredTooLarge>() || error.is::<LengthLimitError>() {
            return Some(anyhow::anyhow!(
                "{node}: a list page exceeds IRONFLOW_MAX_S3_LIST_PAGE_BYTES ({limit})"
            ));
        }
        cause = error.source();
    }
    None
}
