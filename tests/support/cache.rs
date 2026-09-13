use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

pub fn entry_path(directory: &Path, key: &str) -> PathBuf {
    directory.join("v1").join(format!(
        "{}.json",
        hex::encode(Sha256::digest(key.as_bytes()))
    ))
}
