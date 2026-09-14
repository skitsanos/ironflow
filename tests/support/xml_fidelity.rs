use std::io::Write;
use std::path::Path;

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;
use serde_json::{Value, json};

#[path = "pptx_standard.rs"]
mod pptx_standard;

pub const ENCODED: &str = "A&amp;B &#xE9; C<![CDATA[D]]>E";
pub const DECODED: &str = "A&B \u{e9} CDE";

pub fn package(path: &Path, entries: &[(&str, &str)]) {
    let mut zip = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    for (name, xml) in entries {
        zip.start_file(*name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(xml.as_bytes()).unwrap();
    }
    zip.finish().unwrap();
    if entries
        .iter()
        .any(|(name, _)| name.starts_with("ppt/slides/"))
    {
        pptx_standard::complete(path);
    }
}

pub async fn extract(node: &str, path: &Path, format: &str) -> anyhow::Result<Context> {
    execute(
        node,
        json!({"path": path, "format": format,
        "metadata_key": "metadata", "comments_key": "comments"}),
    )
    .await
}

pub async fn execute(node: &str, config: Value) -> anyhow::Result<Context> {
    NodeRegistry::with_builtins()
        .get(node)
        .unwrap()
        .execute(&config, &Context::new())
        .await
}
