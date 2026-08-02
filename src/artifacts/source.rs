use std::fs::File;
use std::path::PathBuf;

use anyhow::Result;

use super::{ArtifactRef, LocalArtifactStore};
use crate::util::execution::ExecutionControl;

/// A file input whose artifact identity is preserved until it is opened on a
/// blocking worker.
#[derive(Clone, Debug)]
pub(crate) enum FileSource {
    Path(PathBuf),
    Artifact(ArtifactRef),
    ArtifactUri(String),
}

pub(crate) struct OpenedFile {
    file: File,
    label: String,
}

impl FileSource {
    pub(crate) fn path(path: impl Into<PathBuf>) -> Self {
        Self::Path(path.into())
    }

    pub(crate) fn artifact(artifact: ArtifactRef) -> Self {
        Self::Artifact(artifact)
    }

    pub(crate) fn artifact_uri(uri: impl Into<String>) -> Self {
        Self::ArtifactUri(uri.into())
    }

    pub(crate) fn open(&self, operation: &str, execution: &ExecutionControl) -> Result<OpenedFile> {
        execution.checkpoint()?;
        let (file, label) = match self {
            Self::Path(path) => (
                crate::util::bounded_read::open_regular_file(path, operation)?,
                path.display().to_string(),
            ),
            Self::Artifact(artifact) => {
                let store = LocalArtifactStore::from_env()?;
                (
                    store.open(artifact, execution)?,
                    artifact.artifact_uri.clone(),
                )
            }
            Self::ArtifactUri(uri) => {
                let store = LocalArtifactStore::from_env()?;
                (store.open_uri(uri, execution)?, uri.clone())
            }
        };
        Ok(OpenedFile { file, label })
    }

    pub(crate) fn file_name(&self) -> String {
        match self {
            Self::Path(path) => path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("file")
                .to_owned(),
            Self::Artifact(artifact) => artifact
                .mime_type
                .as_deref()
                .and_then(extension_for_mime)
                .map(|extension| format!("artifact.{extension}"))
                .unwrap_or_else(|| "artifact".to_owned()),
            Self::ArtifactUri(_) => "artifact".to_owned(),
        }
    }

    pub(crate) fn file_stem(&self, fallback: &str) -> String {
        match self {
            Self::Path(path) => path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or(fallback)
                .to_owned(),
            Self::Artifact(artifact) => artifact.sha256.clone(),
            Self::ArtifactUri(uri) => uri
                .strip_prefix("artifact://sha256/")
                .unwrap_or(fallback)
                .to_owned(),
        }
    }
}

impl OpenedFile {
    pub(crate) fn into_parts(self) -> (File, String) {
        (self.file, self.label)
    }
}

impl From<&std::path::Path> for FileSource {
    fn from(path: &std::path::Path) -> Self {
        Self::Path(path.to_owned())
    }
}

impl From<&PathBuf> for FileSource {
    fn from(path: &PathBuf) -> Self {
        Self::Path(path.clone())
    }
}

impl From<&FileSource> for FileSource {
    fn from(source: &FileSource) -> Self {
        source.clone()
    }
}

fn extension_for_mime(mime: &str) -> Option<&'static str> {
    match mime {
        "audio/mpeg" => Some("mp3"),
        "audio/mp4" => Some("m4a"),
        "audio/wav" | "audio/x-wav" => Some("wav"),
        "video/mp4" => Some("mp4"),
        _ => None,
    }
}
