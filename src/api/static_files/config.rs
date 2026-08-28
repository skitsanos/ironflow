use std::path::PathBuf;

use serde::Deserialize;

/// Optional public static-file hosting configuration for `ironflow serve`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct StaticFilesConfig {
    /// Public root, relative to the process working directory when not absolute.
    pub directory: PathBuf,
    /// Portable file name served for `/` and directory requests.
    pub index: String,
    /// Return the root index for eligible missing browser navigation routes.
    pub spa_fallback: bool,
    /// Negotiate adjacent `.br` and `.gz` files when present.
    pub precompressed: bool,
    /// Optional Cache-Control value added to static responses.
    pub cache_control: Option<String>,
}

impl Default for StaticFilesConfig {
    fn default() -> Self {
        Self {
            directory: PathBuf::from("public"),
            index: "index.html".to_string(),
            spa_fallback: false,
            precompressed: true,
            cache_control: None,
        }
    }
}
