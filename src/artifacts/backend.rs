use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use super::bounded_writer::BoundedArtifactWriter;
use super::remote::S3ArtifactStore;
use super::store::LocalArtifactStore;
use super::{ArtifactRef, DEFAULT_ARTIFACT_DIR};
use crate::util::execution::ExecutionControl;

const BACKEND_ENV: &str = "IRONFLOW_ARTIFACT_BACKEND";

/// Runtime-selected content-addressed artifact store.
#[derive(Clone, Debug)]
pub struct ArtifactStore {
    local: LocalArtifactStore,
    remote: Option<S3ArtifactStore>,
}

impl ArtifactStore {
    /// Create a local-only store rooted at `root`.
    pub fn new(root: impl Into<PathBuf>) -> Result<Self> {
        Ok(Self {
            local: LocalArtifactStore::new(root)?,
            remote: None,
        })
    }

    /// Select `local` (default) or `s3` from `IRONFLOW_ARTIFACT_BACKEND`.
    pub fn from_env() -> Result<Self> {
        let root = std::env::var_os("IRONFLOW_ARTIFACT_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(DEFAULT_ARTIFACT_DIR));
        let local = LocalArtifactStore::new(root)?;
        let backend = std::env::var(BACKEND_ENV).unwrap_or_else(|_| "local".to_owned());
        let remote = match backend.as_str() {
            "local" => None,
            "s3" => Some(S3ArtifactStore::from_env()?),
            _ => bail!("{BACKEND_ENV} must be 'local' or 's3'"),
        };
        Ok(Self { local, remote })
    }

    #[cfg(test)]
    pub(super) fn with_remote_for_test(
        root: impl Into<PathBuf>,
        remote: S3ArtifactStore,
    ) -> Result<Self> {
        Ok(Self {
            local: LocalArtifactStore::new(root)?,
            remote: Some(remote),
        })
    }

    pub fn root(&self) -> &Path {
        self.local.root()
    }

    pub fn put_reader<R: Read>(
        &self,
        reader: R,
        max_bytes: u64,
        mime_type: Option<String>,
        execution: &ExecutionControl,
    ) -> Result<ArtifactRef> {
        let artifact = self
            .local
            .put_reader(reader, max_bytes, mime_type, execution)?;
        self.publish_remote(&artifact, execution)?;
        Ok(artifact)
    }

    pub fn put_path(
        &self,
        source: &Path,
        max_bytes: u64,
        mime_type: Option<String>,
        execution: &ExecutionControl,
    ) -> Result<ArtifactRef> {
        let artifact = self
            .local
            .put_path(source, max_bytes, mime_type, execution)?;
        self.publish_remote(&artifact, execution)?;
        Ok(artifact)
    }

    pub(crate) fn put_writer(
        &self,
        max_bytes: u64,
        mime_type: Option<String>,
        execution: &ExecutionControl,
        write: impl FnOnce(&mut BoundedArtifactWriter<'_>) -> Result<()>,
    ) -> Result<ArtifactRef> {
        let artifact = self
            .local
            .put_writer(max_bytes, mime_type, execution, write)?;
        self.publish_remote(&artifact, execution)?;
        Ok(artifact)
    }

    pub fn resolve_uri(&self, uri: &str) -> Result<PathBuf> {
        self.local.resolve_uri(uri)
    }

    pub fn resolve(&self, artifact: &ArtifactRef) -> Result<PathBuf> {
        self.local.resolve(artifact)
    }

    pub fn open(&self, artifact: &ArtifactRef, execution: &ExecutionControl) -> Result<File> {
        artifact.validate()?;
        match self.local.open(artifact, execution) {
            Ok(file) => Ok(file),
            Err(local_error) => {
                let remote = self.remote.as_ref().ok_or(local_error)?;
                remote
                    .fetch(&self.local, artifact, execution)
                    .with_context(|| format!("failed to restore artifact {}", artifact.sha256))?;
                self.local.open(artifact, execution)
            }
        }
    }

    pub fn open_uri(&self, uri: &str, execution: &ExecutionControl) -> Result<File> {
        match self.local.open_uri(uri, execution) {
            Ok(file) => Ok(file),
            Err(local_error) => {
                let remote = self.remote.as_ref().ok_or(local_error)?;
                let artifact = remote.fetch_uri(&self.local, uri, execution)?;
                self.local.open(&artifact, execution)
            }
        }
    }

    pub(super) fn local(&self) -> &LocalArtifactStore {
        &self.local
    }

    pub(super) fn remote(&self) -> Option<&S3ArtifactStore> {
        self.remote.as_ref()
    }

    fn publish_remote(&self, artifact: &ArtifactRef, execution: &ExecutionControl) -> Result<()> {
        if let Some(remote) = &self.remote {
            let file = self.local.open(artifact, execution)?;
            remote.publish(file, artifact, execution)?;
        }
        Ok(())
    }
}
