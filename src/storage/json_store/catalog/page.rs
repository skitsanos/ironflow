use std::cmp::Ordering;
#[cfg(test)]
use std::sync::atomic::Ordering as AtomicOrdering;

use crate::engine::types::RunSummary;
use crate::storage::run_listing::normalized_started;
use crate::storage::{RunListQuery, RunSummaryPage, StorageError, StorageResult};

use super::CATALOG_NAME;
use super::delta::{self, DELTA_NAME, DeltaOverlay};
use super::format::{self, CatalogRecord, RECORD_BYTES};
use super::header::{self, CatalogHeader, HEADER_BYTES};
use super::state;
use super::transaction;
use crate::storage::json_store::JsonStateStore;

mod merge;

enum PageError {
    Rebuild,
    Store(StorageError),
}

impl From<StorageError> for PageError {
    fn from(error: StorageError) -> Self {
        Self::Store(error)
    }
}

pub(in crate::storage::json_store) async fn list_page(
    store: &JsonStateStore,
    query: &RunListQuery,
) -> StorageResult<RunSummaryPage> {
    if !store.directory.exists().await? {
        return Ok(RunSummaryPage::empty());
    }
    for _ in 0..3 {
        transaction::ensure_current(store).await?;
        let Some(token) = state::current_token(&store.directory).await? else {
            continue;
        };
        match page_once(store, query, &token).await {
            Ok(page) if state::token_unchanged(&store.directory, &token).await? => return Ok(page),
            Ok(_) => continue,
            Err(PageError::Rebuild) => {
                let _lock = state::acquire_lock(&store.directory).await?;
                let current = state::current_token(&store.directory).await?;
                if current.is_none() || current.as_ref() == Some(&token) {
                    store.rebuild_catalog_locked().await?;
                }
            }
            Err(PageError::Store(error)) => {
                if state::token_unchanged(&store.directory, &token).await? {
                    return Err(error);
                }
            }
        }
    }
    Err(StorageError::conflict(
        "JSON run catalog changed repeatedly while reading a page",
    ))
}

async fn page_once(
    store: &JsonStateStore,
    query: &RunListQuery,
    token: &state::CatalogToken,
) -> Result<RunSummaryPage, PageError> {
    let header = read_header(store).await?;
    if header.generation != token.base_generation() {
        return Err(PageError::Rebuild);
    }
    let overlay = read_delta(store).await?;
    if overlay.base_generation != header.generation || overlay.revision != token.delta_revision() {
        return Err(PageError::Rebuild);
    }
    let section = query.status().map_or(0, format::status_section);
    let (section_start, section_count) = header.section(section)?;
    let first = find_first(store, section_start, section_count, query).await?;
    let remaining = section_count.saturating_sub(first);
    let wanted = query.limit().get().saturating_add(1);
    let scan_limit = wanted.saturating_add(overlay.entries().len());
    let count = usize::try_from(remaining)
        .unwrap_or(usize::MAX)
        .min(scan_limit);
    let mut base_records = if count == 0 {
        Vec::new()
    } else {
        read_records(store, section_start + first, count).await?
    };
    validate_page_records(&base_records, query)?;
    base_records = merge::page_records(base_records, overlay.entries(), query, wanted);
    validate_page_records(&base_records, query)?;
    if base_records.is_empty() {
        return Ok(RunSummaryPage::empty());
    }
    let mut summaries = Vec::with_capacity(base_records.len());
    for record in base_records {
        summaries.push(read_indexed_summary(store, &record).await?);
    }
    Ok(RunSummaryPage::from_ordered(summaries, query))
}

async fn read_delta(store: &JsonStateStore) -> Result<DeltaOverlay, PageError> {
    let data = store
        .directory
        .read_regular_prefix(DELTA_NAME, delta::MAX_BYTES + 1)
        .await?
        .ok_or(PageError::Rebuild)?;
    #[cfg(test)]
    {
        store
            .catalog_io
            .delta_reads
            .fetch_add(1, AtomicOrdering::Relaxed);
        store
            .catalog_io
            .delta_read_bytes
            .fetch_add(data.len(), AtomicOrdering::Relaxed);
    }
    delta::decode(&data).map_err(|_| PageError::Rebuild)
}

async fn read_header(store: &JsonStateStore) -> Result<CatalogHeader, PageError> {
    let data = store
        .directory
        .read_regular_prefix(CATALOG_NAME, HEADER_BYTES)
        .await?
        .ok_or(PageError::Rebuild)?;
    let metadata = tokio::fs::symlink_metadata(store.directory.path(CATALOG_NAME))
        .await
        .map_err(|_| PageError::Rebuild)?;
    header::decode(&data, metadata.len()).map_err(|_| PageError::Rebuild)
}

async fn find_first(
    store: &JsonStateStore,
    section_start: u64,
    section_count: u64,
    query: &RunListQuery,
) -> Result<u64, PageError> {
    let Some(cursor) = query.after() else {
        return Ok(0);
    };
    let mut low = 0;
    let mut high = section_count;
    while low < high {
        let middle = low + (high - low) / 2;
        let record = read_record(store, section_start + middle).await?;
        let ordering = normalized_started(cursor.started())
            .cmp(&normalized_started(record.started))
            .then_with(|| cursor.id().cmp(&record.id));
        if ordering == Ordering::Greater {
            high = middle;
        } else {
            low = middle + 1;
        }
    }
    Ok(low)
}

async fn read_record(
    store: &JsonStateStore,
    record_index: u64,
) -> Result<CatalogRecord, PageError> {
    let offset = record_offset(record_index).ok_or(PageError::Rebuild)?;
    let data = store
        .directory
        .read_regular_range(CATALOG_NAME, offset, RECORD_BYTES)
        .await?
        .ok_or(PageError::Rebuild)?;
    #[cfg(test)]
    store
        .catalog_io
        .base_point_records
        .fetch_add(1, AtomicOrdering::Relaxed);
    format::decode_record(&data).map_err(|_| PageError::Rebuild)
}

async fn read_records(
    store: &JsonStateStore,
    first: u64,
    count: usize,
) -> Result<Vec<CatalogRecord>, PageError> {
    let offset = record_offset(first).ok_or(PageError::Rebuild)?;
    let length = count.checked_mul(RECORD_BYTES).ok_or(PageError::Rebuild)?;
    let data = store
        .directory
        .read_regular_range(CATALOG_NAME, offset, length)
        .await?
        .ok_or(PageError::Rebuild)?;
    #[cfg(test)]
    store
        .catalog_io
        .base_range_records
        .fetch_add(count, AtomicOrdering::Relaxed);
    data.chunks_exact(RECORD_BYTES)
        .map(|record| format::decode_record(record).map_err(|_| PageError::Rebuild))
        .collect()
}

async fn read_indexed_summary(
    store: &JsonStateStore,
    indexed: &CatalogRecord,
) -> Result<RunSummary, PageError> {
    #[cfg(test)]
    store.wait_catalog_read_hook().await;
    let summary = store.read_current_summary(&indexed.id).await?;
    if summary.id != indexed.id
        || summary.status != indexed.status
        || normalized_started(summary.started) != normalized_started(indexed.started)
    {
        return Err(PageError::Rebuild);
    }
    Ok(summary)
}

fn validate_page_records(records: &[CatalogRecord], query: &RunListQuery) -> Result<(), PageError> {
    if records.windows(2).any(|pair| {
        format::compare_records(&pair[0], &pair[1]) != Ordering::Less || pair[0].id == pair[1].id
    }) || records.iter().any(|record| {
        query
            .status()
            .is_some_and(|status| record.status != *status)
    }) {
        return Err(PageError::Rebuild);
    }
    Ok(())
}

fn record_offset(index: u64) -> Option<u64> {
    index
        .checked_mul(RECORD_BYTES as u64)?
        .checked_add(HEADER_BYTES as u64)
}
