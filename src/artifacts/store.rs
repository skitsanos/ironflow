use std::fs::File;
use std::io::{Read, Seek, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use sha2::{Digest as _, Sha256};
use uuid::Uuid;

use crate::util::execution::ExecutionControl;

use super::ArtifactRef;
use super::bounded_writer::BoundedArtifactWriter;
use super::filesystem::{self, TempArtifact};
use super::integrity::{hash_reader, verify_existing};
use super::reference::digest_from_uri;

pub const DEFAULT_ARTIFACT_DIR: &str = "data/artifacts";
const DIGEST_DIRECTORY: &str = "sha256";
const COPY_CHUNK_BYTES: usize = 16 * 1024;

/// A local, content-addressed store for immutable workflow artifacts.
#[derive(Clone, Debug)]
pub struct LocalArtifactStore {
    root: PathBuf,
    digest_directory: PathBuf,
}

impl LocalArtifactStore {
    /// Create or validate a store rooted at `root`.
    pub fn new(root: impl Into<PathBuf>) -> Result<Self> {
        let configured_root = root.into();
        if configured_root.as_os_str().is_empty() {
            bail!("artifact directory cannot be empty");
        }
        filesystem::ensure_private_directory(&configured_root)?;
        let root = std::fs::canonicalize(&configured_root).with_context(|| {
            format!(
                "failed to resolve artifact directory '{}'",
                configured_root.display()
            )
        })?;
        let digest_directory = root.join(DIGEST_DIRECTORY);
        filesystem::ensure_private_directory(&digest_directory)?;
        Ok(Self {
            root,
            digest_directory,
        })
    }

    /// Use `IRONFLOW_ARTIFACT_DIR`, or `data/artifacts` when it is unset.
    pub fn from_env() -> Result<Self> {
        let root = std::env::var_os("IRONFLOW_ARTIFACT_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(DEFAULT_ARTIFACT_DIR));
        Self::new(root)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Stream a reader into the store without retaining its payload in memory.
    pub fn put_reader<R: Read>(
        &self,
        mut reader: R,
        max_bytes: u64,
        mime_type: Option<String>,
        execution: &ExecutionControl,
    ) -> Result<ArtifactRef> {
        execution.checkpoint()?;
        super::reference::validate_mime_type(mime_type.as_deref())?;
        let mut temporary = self.create_temporary()?;
        let mut hasher = Sha256::new();
        let mut size = 0_u64;
        let mut chunk = [0_u8; COPY_CHUNK_BYTES];

        loop {
            execution.checkpoint()?;
            let read = reader
                .read(&mut chunk)
                .context("failed to read artifact source")?;
            if read == 0 {
                break;
            }
            size = size
                .checked_add(read as u64)
                .context("artifact size overflow")?;
            if size > max_bytes {
                bail!("artifact exceeds the {max_bytes} byte limit");
            }
            hasher.update(&chunk[..read]);
            temporary
                .file_mut()
                .write_all(&chunk[..read])
                .context("failed to write artifact temporary file")?;
        }

        execution.checkpoint()?;
        temporary
            .file_mut()
            .flush()
            .context("failed to flush artifact")?;
        temporary
            .file()
            .sync_all()
            .context("failed to sync artifact")?;
        filesystem::harden_staging_file(temporary.file())?;
        let digest = hex::encode(hasher.finalize());
        let destination = self.digest_directory.join(&digest);
        self.publish(temporary, &destination, &digest, size, execution)?;
        ArtifactRef::from_digest(digest, size, mime_type)
    }

    /// Stream a regular file into the store without following a final symlink.
    pub fn put_path(
        &self,
        source: &Path,
        max_bytes: u64,
        mime_type: Option<String>,
        execution: &ExecutionControl,
    ) -> Result<ArtifactRef> {
        execution.checkpoint()?;
        let file = crate::util::bounded_read::open_regular_file(source, "artifact source")?;
        let declared = file.metadata()?.len();
        if declared > max_bytes {
            bail!(
                "artifact source '{}' is {declared} bytes, exceeds the {max_bytes} byte limit",
                source.display()
            );
        }
        self.put_reader(file, max_bytes, mime_type, execution)
    }

    /// Generate a seekable file directly in the store's private staging area.
    ///
    /// This is intended for encoders that require `Write + Seek`; their output
    /// is hashed from disk and published without an intermediate byte vector.
    pub(crate) fn put_writer(
        &self,
        max_bytes: u64,
        mime_type: Option<String>,
        execution: &ExecutionControl,
        write: impl FnOnce(&mut BoundedArtifactWriter<'_>) -> Result<()>,
    ) -> Result<ArtifactRef> {
        execution.checkpoint()?;
        super::reference::validate_mime_type(mime_type.as_deref())?;
        let mut temporary = self.create_temporary()?;
        let size = {
            let mut writer =
                BoundedArtifactWriter::new(temporary.file_mut(), max_bytes, execution)?;
            write(&mut writer)
                .map_err(|error| anyhow::anyhow!("failed to generate artifact: {error:#}"))?;
            writer.flush().context("failed to flush artifact")?;
            writer.len()
        };
        execution.checkpoint()?;
        temporary
            .file()
            .sync_all()
            .context("failed to sync artifact")?;
        if temporary.file().metadata()?.len() != size {
            bail!("generated artifact length changed before hashing");
        }
        temporary.file_mut().rewind()?;
        let digest = hash_reader(temporary.file_mut(), size, execution)?;
        filesystem::harden_staging_file(temporary.file())?;
        let destination = self.digest_directory.join(&digest);
        self.publish(temporary, &destination, &digest, size, execution)?;
        ArtifactRef::from_digest(digest, size, mime_type)
    }

    /// Resolve a canonical artifact URI for administrative inspection.
    ///
    /// Consumers must use [`Self::open_uri`] so the verified handle, rather
    /// than this mutable pathname, is passed to the parser.
    pub fn resolve_uri(&self, uri: &str) -> Result<PathBuf> {
        let digest = digest_from_uri(uri)?;
        let path = self.digest_directory.join(digest);
        let file = crate::util::bounded_read::open_regular_file(&path, "artifact")?;
        drop(file);
        Ok(path)
    }

    /// Resolve and size-check a descriptor for administrative inspection.
    ///
    /// This does not authenticate bytes at the returned mutable pathname.
    /// Consumers must use [`Self::open`] or [`Self::verified_path_lease`].
    pub fn resolve(&self, artifact: &ArtifactRef) -> Result<PathBuf> {
        artifact.validate()?;
        let path = self.resolve_uri(&artifact.artifact_uri)?;
        let actual = std::fs::metadata(&path)?.len();
        if actual != artifact.size_bytes {
            bail!(
                "artifact size mismatch: descriptor says {}, stored file is {actual}",
                artifact.size_bytes
            );
        }
        Ok(path)
    }

    /// Open and cryptographically verify a descriptor, returning the same
    /// rewound handle that was hashed.
    pub fn open(&self, artifact: &ArtifactRef, execution: &ExecutionControl) -> Result<File> {
        artifact.validate()?;
        self.open_digest(&artifact.sha256, Some(artifact.size_bytes), execution)
    }

    /// Open and verify a canonical artifact URI, returning the same rewound
    /// handle that was hashed.
    pub fn open_uri(&self, uri: &str, execution: &ExecutionControl) -> Result<File> {
        let digest = digest_from_uri(uri)?;
        self.open_digest(digest, None, execution)
    }

    fn open_digest(
        &self,
        digest: &str,
        expected_size: Option<u64>,
        execution: &ExecutionControl,
    ) -> Result<File> {
        execution.checkpoint()?;
        let path = self.digest_directory.join(digest);
        let mut file = crate::util::bounded_read::open_regular_file(&path, "artifact")?;
        let actual = file.metadata()?.len();
        if expected_size.is_some_and(|expected| expected != actual) {
            bail!(
                "artifact size mismatch: descriptor says {}, stored file is {actual}",
                expected_size.expect("checked as present")
            );
        }
        let computed = hash_reader(&mut file, actual, execution)?;
        if computed != digest {
            bail!("stored artifact failed digest verification");
        }
        file.rewind()?;
        execution.checkpoint()?;
        Ok(file)
    }

    pub(super) fn create_temporary(&self) -> Result<TempArtifact> {
        for _ in 0..8 {
            let path = self
                .digest_directory
                .join(format!(".ironflow-artifact-{}.tmp", Uuid::new_v4()));
            match filesystem::create_private_file(&path) {
                Ok(file) => return Ok(TempArtifact::new(path, file)),
                Err(error) if filesystem::is_already_exists(&error) => continue,
                Err(error) => {
                    return Err(error).with_context(|| {
                        format!(
                            "failed to create artifact temporary file '{}'",
                            path.display()
                        )
                    });
                }
            }
        }
        bail!("failed to allocate a unique artifact temporary file")
    }

    fn publish(
        &self,
        temporary: TempArtifact,
        destination: &Path,
        digest: &str,
        size: u64,
        execution: &ExecutionControl,
    ) -> Result<()> {
        execution.checkpoint()?;
        match std::fs::hard_link(temporary.path(), destination) {
            Ok(()) => {
                temporary.remove()?;
                if let Err(error) = filesystem::harden_published_path(destination) {
                    // The destination was created by this call and has not
                    // escaped as a successful descriptor. Best-effort removal
                    // avoids leaving a writable content-addressed entry.
                    filesystem::remove_failed_publication(destination);
                    return Err(error);
                }
                filesystem::sync_directory(&self.digest_directory)
            }
            Err(error) if filesystem::is_already_exists(&error) => {
                verify_existing(destination, digest, size, execution)?;
                temporary.remove()
            }
            Err(error) => Err(error)
                .with_context(|| format!("failed to publish artifact '{}'", destination.display())),
        }
    }
}
