use std::io::{Seek, Write};
use std::path::Path;

pub const PML: &str = "http://schemas.openxmlformats.org/presentationml/2006/main";
pub const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

pub fn add<W: Write + Seek>(zip: &mut zip::ZipWriter<W>, name: &str, bytes: &[u8]) {
    zip.start_file(
        name,
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored),
    )
    .unwrap();
    zip.write_all(bytes).unwrap();
}

pub fn presentation(ids: &[(&str, &str)]) -> String {
    let slides = ids
        .iter()
        .map(|(id, rid)| format!("<p:sldId id=\"{id}\" r:id=\"{rid}\"/>"))
        .collect::<String>();
    format!(
        "<p:presentation xmlns:p=\"{PML}\" xmlns:r=\"{REL}\"><p:sldIdLst>{slides}</p:sldIdLst></p:presentation>"
    )
}

pub fn relationships(entries: &[(&str, &str, &str)]) -> String {
    let body = entries
        .iter()
        .map(|(id, kind, target)| {
            format!("<Relationship Id=\"{id}\" Type=\"{REL}/{kind}\" Target=\"{target}\"/>")
        })
        .collect::<String>();
    format!(
        "<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">{body}</Relationships>"
    )
}

pub fn slide(text: &str) -> String {
    format!(
        "<p:sld xmlns:p=\"{PML}\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\"><p:sp><p:txBody><a:p><a:r><a:t>{text}</a:t></a:r></a:p></p:txBody></p:sp></p:sld>"
    )
}

pub fn write(path: &Path, parts: &[(&str, &str)]) {
    let mut zip = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    for (name, value) in parts {
        add(&mut zip, name, value.as_bytes());
    }
    zip.finish().unwrap();
}
