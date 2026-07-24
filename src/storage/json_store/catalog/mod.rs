//! Crash-safe fixed-record index for JSON run-summary pages.
//!
//! Global and per-status sections make steady pages `O(log N + page size)`
//! without directory enumeration. Inserts, status changes, and deletes rewrite
//! the `O(N)` projection atomically; task/context-only updates keep the catalog
//! generation and file unchanged and refresh only its small clean-state stamp.

mod format;
mod header;
mod page;
mod state;
mod transaction;

#[cfg(test)]
mod ordering_tests;
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
