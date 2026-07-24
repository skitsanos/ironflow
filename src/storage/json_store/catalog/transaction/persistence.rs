use std::collections::BTreeMap;
#[cfg(test)]
use std::sync::atomic::Ordering;

use crate::storage::{StorageError, StorageResult};

use super::super::CATALOG_NAME;
use super::super::delta::{self, DELTA_NAME, DeltaEntry, DeltaOverlay};
use super::super::format::{self, CatalogRecord, RECORD_BYTES};
use super::super::header::{self, HEADER_BYTES};
use super::super::state::{self, CatalogToken};
use crate::storage::json_store::JsonStateStore;

pub(super) async fn commit_mutation(
    store: &JsonStateStore,
    token: &CatalogToken,
    mutation: DeltaEntry,
) -> StorageResult<()> {
    let overlay = read_overlay(store, token).await?;
    let mut latest = overlay
        .into_entries()
        .into_iter()
        .map(|entry| (entry.id().to_string(), entry))
        .collect::<BTreeMap<_, _>>();
    latest.insert(mutation.id().to_string(), mutation);

    if latest.len() <= delta::MAX_ENTRIES {
        return replace_delta(store, token.base_generation(), latest.into_values()).await;
    }

    let mut base = read_base_records(store, token.base_generation()).await?;
    base.retain(|record| !latest.contains_key(&record.id));
    let mut changed = latest
        .into_values()
        .filter_map(|entry| match entry {
            DeltaEntry::Upsert(record) => Some(record),
            DeltaEntry::Delete(_) => None,
        })
        .collect::<Vec<_>>();
    changed.sort_by(format::compare_records);
    let records = merge_ordered(base, changed);
    replace_snapshot(store, records, true).await?;
    Ok(())
}

fn merge_ordered(base: Vec<CatalogRecord>, changed: Vec<CatalogRecord>) -> Vec<CatalogRecord> {
    let capacity = base.len().saturating_add(changed.len());
    let mut base = base.into_iter().peekable();
    let mut changed = changed.into_iter().peekable();
    let mut merged = Vec::with_capacity(capacity);
    loop {
        let next = match (base.peek(), changed.peek()) {
            (Some(base_record), Some(changed_record)) => {
                if format::compare_records(base_record, changed_record) == std::cmp::Ordering::Less
                {
                    base.next()
                } else {
                    changed.next()
                }
            }
            (Some(_), None) => base.next(),
            (None, Some(_)) => changed.next(),
            (None, None) => break,
        };
        merged.push(next.expect("a selected compaction merge branch contains a record"));
    }
    merged
}

async fn read_overlay(store: &JsonStateStore, token: &CatalogToken) -> StorageResult<DeltaOverlay> {
    let data = store
        .directory
        .read_regular_prefix(DELTA_NAME, delta::MAX_BYTES + 1)
        .await?
        .ok_or_else(|| corrupt("catalog delta is missing"))?;
    #[cfg(test)]
    {
        store.catalog_io.delta_reads.fetch_add(1, Ordering::Relaxed);
        store
            .catalog_io
            .delta_read_bytes
            .fetch_add(data.len(), Ordering::Relaxed);
    }
    let overlay = delta::decode(&data)?;
    if overlay.base_generation != token.base_generation()
        || overlay.revision != token.delta_revision()
    {
        return Err(corrupt("catalog delta changed during the transaction"));
    }
    Ok(overlay)
}

async fn replace_delta(
    store: &JsonStateStore,
    base_generation: uuid::Uuid,
    entries: impl IntoIterator<Item = DeltaEntry>,
) -> StorageResult<()> {
    let (revision, data) = delta::encode(base_generation, entries)?;
    store.directory.write_replace(DELTA_NAME, &data).await?;
    #[cfg(test)]
    {
        store
            .catalog_io
            .delta_replacements
            .fetch_add(1, Ordering::Relaxed);
        store
            .catalog_io
            .delta_write_bytes
            .fetch_add(data.len(), Ordering::Relaxed);
    }
    state::mark_clean(&store.directory, base_generation, revision).await?;
    Ok(())
}

async fn read_base_records(
    store: &JsonStateStore,
    expected_generation: uuid::Uuid,
) -> StorageResult<Vec<CatalogRecord>> {
    let data = store
        .directory
        .read_regular(CATALOG_NAME)
        .await?
        .ok_or_else(|| corrupt("catalog base is missing"))?;
    #[cfg(test)]
    {
        store
            .catalog_io
            .base_full_reads
            .fetch_add(1, Ordering::Relaxed);
        store
            .catalog_io
            .base_read_bytes
            .fetch_add(data.len(), Ordering::Relaxed);
    }
    let header = header::decode(&data[..data.len().min(HEADER_BYTES)], data.len() as u64)?;
    if header.generation != expected_generation {
        return Err(corrupt("catalog base changed during compaction"));
    }
    let (_, count) = header.section(0)?;
    let count = usize::try_from(count)
        .map_err(|error| StorageError::corruption("Invalid JSON run catalog", error))?;
    let mut records = Vec::with_capacity(count);
    for index in 0..count {
        let start = HEADER_BYTES + index * RECORD_BYTES;
        records.push(format::decode_record(&data[start..start + RECORD_BYTES])?);
    }
    if records.windows(2).any(|pair| {
        format::compare_records(&pair[0], &pair[1]) != std::cmp::Ordering::Less
            || pair[0].id == pair[1].id
    }) {
        return Err(corrupt("catalog global section is unordered or duplicated"));
    }
    Ok(records)
}

pub(super) async fn replace_snapshot(
    store: &JsonStateStore,
    mut records: Vec<CatalogRecord>,
    compaction: bool,
) -> StorageResult<()> {
    let (base_generation, base_data) = format::encode(records.as_mut_slice())?;
    let (delta_revision, delta_data) = delta::encode(base_generation, [])?;
    store
        .directory
        .write_replace(CATALOG_NAME, &base_data)
        .await?;
    #[cfg(test)]
    {
        store
            .catalog_io
            .base_replacements
            .fetch_add(1, Ordering::Relaxed);
        store
            .catalog_io
            .base_write_bytes
            .fetch_add(base_data.len(), Ordering::Relaxed);
    }
    store
        .directory
        .write_replace(DELTA_NAME, &delta_data)
        .await?;
    #[cfg(test)]
    {
        store
            .catalog_io
            .delta_replacements
            .fetch_add(1, Ordering::Relaxed);
        store
            .catalog_io
            .delta_write_bytes
            .fetch_add(delta_data.len(), Ordering::Relaxed);
        if compaction {
            store.catalog_io.compactions.fetch_add(1, Ordering::Relaxed);
        }
    }
    #[cfg(not(test))]
    let _ = compaction;
    state::mark_clean(&store.directory, base_generation, delta_revision).await?;
    Ok(())
}

fn corrupt(detail: &'static str) -> StorageError {
    StorageError::corruption("Invalid JSON run catalog", detail)
}
