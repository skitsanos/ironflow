use std::cmp::Ordering;
use std::num::NonZeroUsize;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::engine::types::{RunStatus, RunSummary};

use super::{StorageError, StorageResult};
use crate::storage::run_id::validate_run_id;

const RUN_CURSOR_VERSION: u8 = 1;
const MAX_ENCODED_CURSOR_BYTES: usize = 1_024;

/// A validated, non-zero number of records requested from a state store.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageSize(NonZeroUsize);

impl PageSize {
    pub fn new(value: usize) -> StorageResult<Self> {
        NonZeroUsize::new(value)
            .filter(|value| value.get() < usize::MAX)
            .map(Self)
            .ok_or_else(|| {
                StorageError::invalid_input(
                    "list page size must be greater than zero and leave room for a continuation",
                )
            })
    }

    pub const fn get(self) -> usize {
        self.0.get()
    }
}

/// Opaque keyset cursor returned by a run-summary page.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunCursor {
    version: u8,
    status: Option<RunStatus>,
    started: Option<DateTime<Utc>>,
    id: String,
}

impl RunCursor {
    fn from_summary(summary: &RunSummary, status: Option<RunStatus>) -> Self {
        Self {
            version: RUN_CURSOR_VERSION,
            status,
            started: summary.started,
            id: summary.id.clone(),
        }
    }

    pub fn encode(&self) -> StorageResult<String> {
        let payload = serde_json::to_vec(self)
            .map_err(|error| StorageError::backend("Failed to encode run cursor", error))?;
        Ok(URL_SAFE_NO_PAD.encode(payload))
    }

    pub fn decode(encoded: &str) -> StorageResult<Self> {
        if encoded.is_empty() || encoded.len() > MAX_ENCODED_CURSOR_BYTES {
            return Err(StorageError::invalid_input("invalid run-list cursor"));
        }
        let payload = URL_SAFE_NO_PAD
            .decode(encoded)
            .map_err(|_| StorageError::invalid_input("invalid run-list cursor"))?;
        let cursor: Self = serde_json::from_slice(&payload)
            .map_err(|_| StorageError::invalid_input("invalid run-list cursor"))?;
        if cursor.version != RUN_CURSOR_VERSION {
            return Err(StorageError::invalid_input("invalid run-list cursor"));
        }
        validate_run_id(&cursor.id)
            .map_err(|_| StorageError::invalid_input("invalid run-list cursor"))?;
        Ok(cursor)
    }

    pub const fn started(&self) -> Option<DateTime<Utc>> {
        self.started
    }

    pub fn id(&self) -> &str {
        &self.id
    }
}

/// A bounded newest-first run-summary query.
#[derive(Clone, Debug)]
pub struct RunListQuery {
    status: Option<RunStatus>,
    after: Option<RunCursor>,
    limit: PageSize,
}

impl RunListQuery {
    pub fn new(
        status: Option<RunStatus>,
        after: Option<RunCursor>,
        limit: PageSize,
    ) -> StorageResult<Self> {
        if after
            .as_ref()
            .is_some_and(|cursor| validate_run_id(&cursor.id).is_err())
        {
            return Err(StorageError::invalid_input("invalid run-list cursor"));
        }
        if after.as_ref().is_some_and(|cursor| cursor.status != status) {
            return Err(StorageError::invalid_input(
                "run-list cursor does not match the status filter",
            ));
        }
        Ok(Self {
            status,
            after,
            limit,
        })
    }

    pub fn status(&self) -> Option<&RunStatus> {
        self.status.as_ref()
    }

    pub fn after(&self) -> Option<&RunCursor> {
        self.after.as_ref()
    }

    pub const fn limit(&self) -> PageSize {
        self.limit
    }
}

/// One bounded run-summary page plus an optional cursor for the next page.
#[derive(Clone, Debug)]
pub struct RunSummaryPage {
    pub items: Vec<RunSummary>,
    pub next: Option<RunCursor>,
}

impl RunSummaryPage {
    pub fn empty() -> Self {
        Self {
            items: Vec::new(),
            next: None,
        }
    }

    pub fn has_more(&self) -> bool {
        self.next.is_some()
    }

    pub(crate) fn from_ordered(mut items: Vec<RunSummary>, query: &RunListQuery) -> Self {
        let has_more = items.len() > query.limit.get();
        items.truncate(query.limit.get());
        let next = has_more.then(|| {
            RunCursor::from_summary(
                items
                    .last()
                    .expect("a page with a continuation has at least one item"),
                query.status.clone(),
            )
        });
        Self { items, next }
    }
}

/// Comparator for the public order: newest timestamp first, missing timestamp
/// last, then lexicographically greatest run ID first.
pub(crate) fn compare_summaries(left: &RunSummary, right: &RunSummary) -> Ordering {
    normalized_started(right.started)
        .cmp(&normalized_started(left.started))
        .then_with(|| right.id.cmp(&left.id))
}

pub(crate) fn normalized_started(started: Option<DateTime<Utc>>) -> Option<i64> {
    started.map(|started| started.timestamp_micros())
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone as _, Utc};

    use super::*;

    fn summary(id: &str, second: Option<u32>, status: RunStatus) -> RunSummary {
        RunSummary {
            id: id.to_string(),
            flow_name: "flow".to_string(),
            status,
            started: second.map(|second| Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, second).unwrap()),
            finished: None,
            task_count: 0,
        }
    }

    #[test]
    fn cursor_round_trip_is_filter_bound_and_versioned() {
        let item = summary("run-1", Some(1), RunStatus::Failed);
        let cursor = RunCursor::from_summary(&item, Some(RunStatus::Failed));
        assert_eq!(
            RunCursor::decode(&cursor.encode().unwrap()).unwrap(),
            cursor
        );

        let error = RunListQuery::new(
            Some(RunStatus::Success),
            Some(cursor),
            PageSize::new(1).unwrap(),
        )
        .unwrap_err();
        assert_eq!(error.kind(), super::super::StorageErrorKind::InvalidInput);
        assert!(RunCursor::decode("not-a-cursor").is_err());
        let invalid = RunCursor {
            version: RUN_CURSOR_VERSION,
            status: None,
            started: None,
            id: "../outside".to_string(),
        };
        assert!(RunCursor::decode(&invalid.encode().unwrap()).is_err());
    }
}
