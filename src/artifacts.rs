//! Content-addressed artifacts for large workflow payloads.

mod backend;
mod bounded_writer;
mod filesystem;
mod integrity;
mod lease;
mod publication;
mod reference;
mod remote;
mod remote_config;
mod remote_retention;
mod retention;
mod source;
mod store;

pub use backend::ArtifactStore;
pub use lease::VerifiedPathLease;
pub use reference::ArtifactRef;
pub(crate) use reference::validate_mime_type;
pub(crate) use retention::ArtifactCandidate;
pub(crate) use source::FileSource;
pub use store::DEFAULT_ARTIFACT_DIR;

/// Backwards-compatible name for the runtime-selected artifact store.
///
/// [`Self::new`] always creates a local-only store. [`Self::from_env`] selects
/// the configured local or S3-compatible backend.
pub type LocalArtifactStore = ArtifactStore;

#[cfg(test)]
mod tests;
