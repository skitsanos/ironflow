//! Disk-backed artifacts for large workflow payloads.

mod bounded_writer;
mod filesystem;
mod integrity;
mod lease;
mod reference;
mod source;
mod store;

pub use lease::VerifiedPathLease;
pub use reference::ArtifactRef;
pub(crate) use reference::validate_mime_type;
pub(crate) use source::FileSource;
pub use store::{DEFAULT_ARTIFACT_DIR, LocalArtifactStore};

#[cfg(test)]
mod tests;
