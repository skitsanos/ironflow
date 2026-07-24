//! Crash-safe fixed-record index for JSON run-summary pages.
//!
//! Global and per-status base sections plus a bounded mutation overlay make
//! steady pages `O(log N + page size + K)` without directory enumeration,
//! where `K <= 128`. Projection-changing writes atomically replace only the
//! coalesced overlay until its next distinct ID triggers `O(N)` compaction;
//! task/context-only updates leave both projection files unchanged.

mod delta;
mod format;
mod header;
mod page;
mod state;
mod transaction;

#[cfg(test)]
mod benchmark_tests;
#[cfg(test)]
mod complexity_tests;
#[cfg(test)]
mod ordering_tests;
#[cfg(test)]
mod recovery_concurrency_tests;
#[cfg(test)]
mod resilience_tests;
#[cfg(test)]
mod tests;

pub(super) use format::CatalogRecord;
pub(super) use page::list_page;
pub(super) use transaction::CatalogTransaction;

pub(super) const CATALOG_NAME: &str = ".ironflow-run-catalog-v1.bin";
pub(super) const STATE_NAME: &str = ".ironflow-run-catalog-v1.state";
pub(super) const LOCK_NAME: &str = ".ironflow-run-catalog-v1.lock";

const SECTION_COUNT: usize = 7;
