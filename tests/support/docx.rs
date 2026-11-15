//! Hand-built DOCX packages for extraction tests: raw WordprocessingML parts
//! zipped under their package paths.

use std::io::Write;

use ironflow::{engine::types::Context, nodes::NodeRegistry};

pub const NAMESPACES: &str = "xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" \
    xmlns:mc=\"http://schemas.openxmlformats.org/markup-compatibility/2006\" \
    xmlns:wp=\"http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing\" \
    xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" \
    xmlns:wps=\"http://schemas.microsoft.com/office/word/2010/wordprocessingShape\" \
    xmlns:v=\"urn:schemas-microsoft-com:vml\"";

const DECLARATION: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>";

/// `word/document.xml` with `body` as the content of `w:body`.
pub fn document(body: &str) -> String {
    format!("{DECLARATION}<w:document {NAMESPACES}><w:body>{body}</w:body></w:document>")
}

/// A part with root element `root` (for example `w:styles`) around `content`.
pub fn part(root: &str, content: &str) -> String {
    format!("{DECLARATION}<{root} {NAMESPACES}>{content}</{root}>")
}

/// Run `extract_word` on a package made of `parts` (package path, XML) with
/// `config` merged over the generated `path`.
pub async fn extract(
    parts: &[(&str, String)],
    mut config: serde_json::Value,
) -> ironflow::engine::types::NodeOutput {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("fixture.docx");
    let mut archive = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
    for (name, xml) in parts {
        archive
            .start_file(*name, zip::write::SimpleFileOptions::default())
            .unwrap();
        archive.write_all(xml.as_bytes()).unwrap();
    }
    archive.finish().unwrap();
    config["path"] = serde_json::json!(path);
    NodeRegistry::with_builtins()
        .get("extract_word")
        .unwrap()
        .execute(&config, &Context::new())
        .await
        .unwrap()
}
