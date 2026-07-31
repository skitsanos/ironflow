//! Disk-backed artifacts for large workflow payloads.

mod bounded_writer;
mod filesystem;
mod integrity;
mod reference;
mod store;

pub use reference::ArtifactRef;
pub(crate) use reference::validate_mime_type;
pub use store::{DEFAULT_ARTIFACT_DIR, LocalArtifactStore};

#[cfg(test)]
mod tests;
