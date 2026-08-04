use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::Serialize;
use sha2::{Digest, Sha256};
use zip::write::SimpleFileOptions;

#[path = "fixtures/pdf.rs"]
mod pdf;

#[derive(Serialize)]
pub(crate) struct Fixture {
    label: &'static str,
    node: &'static str,
    path: PathBuf,
    sha256: String,
    raw_bytes: u64,
    declared_bytes: u64,
}

pub(crate) fn generate(output: &Path) -> Result<Vec<Fixture>> {
    std::fs::create_dir_all(output)?;
    let cases = [
        (
            "long_zip_metadata",
            "extract_word",
            write_long_metadata_docx(output)?,
        ),
        (
            "repeated_pptx_media",
            "extract_pptx",
            write_repeated_media_pptx(output)?,
        ),
        (
            "compressed_pdf_text",
            "extract_pdf",
            pdf::write_compressed(output)?,
        ),
        (
            "cid_unicode_pdf",
            "extract_pdf",
            pdf::write_cid_unicode(output)?,
        ),
        (
            "pathological_html",
            "extract_html",
            write_pathological_html(output)?,
        ),
    ];
    cases
        .into_iter()
        .map(|(label, node, path)| inspect(label, node, path))
        .collect()
}

fn inspect(label: &'static str, node: &'static str, path: PathBuf) -> Result<Fixture> {
    let bytes = std::fs::read(&path)?;
    let raw_bytes = bytes.len() as u64;
    let declared_bytes = if matches!(
        path.extension().and_then(|value| value.to_str()),
        Some("docx" | "pptx")
    ) {
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(&bytes))?;
        let mut total = 0_u64;
        for index in 0..archive.len() {
            total = total.saturating_add(archive.by_index_raw(index)?.size());
        }
        total
    } else {
        raw_bytes
    };
    Ok(Fixture {
        label,
        node,
        path,
        sha256: hex::encode(Sha256::digest(&bytes)),
        raw_bytes,
        declared_bytes,
    })
}

fn zip_options() -> SimpleFileOptions {
    SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .last_modified_time(zip::DateTime::default())
}

fn write_long_metadata_docx(output: &Path) -> Result<PathBuf> {
    let path = output.join("long-zip-metadata.docx");
    let mut archive = zip::ZipWriter::new(std::fs::File::create(&path)?);
    archive.start_file("word/document.xml", zip_options())?;
    archive.write_all(
        br#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>bounded benchmark</w:t></w:r></w:p></w:body></w:document>"#,
    )?;
    for index in 0..192 {
        let name = format!(
            "customXml/metadata-{index:03}-{}.xml",
            "long-name-segment-".repeat(7)
        );
        archive.start_file(name, zip_options())?;
        archive.write_all(b"<metadata/>")?;
    }
    archive.finish()?;
    Ok(path)
}

fn write_repeated_media_pptx(output: &Path) -> Result<PathBuf> {
    let path = output.join("repeated-media.pptx");
    let mut archive = zip::ZipWriter::new(std::fs::File::create(&path)?);
    let relationship_type =
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image";
    for index in 1..=48 {
        archive.start_file(format!("ppt/slides/slide{index}.xml"), zip_options())?;
        let pictures = (0..8)
            .map(|picture| {
                format!("<pic><cNvPr descr=\"image-{picture}\"/><blip embed=\"rId1\"/></pic>")
            })
            .collect::<String>();
        archive.write_all(format!("<sld>{pictures}</sld>").as_bytes())?;
        archive.start_file(
            format!("ppt/slides/_rels/slide{index}.xml.rels"),
            zip_options(),
        )?;
        archive.write_all(
            format!(
                "<Relationships><Relationship Id=\"rId1\" Type=\"{relationship_type}\" Target=\"../media/image.png\"/></Relationships>"
            )
            .as_bytes(),
        )?;
    }
    archive.start_file("ppt/media/image.png", zip_options())?;
    archive.write_all(b"deterministic-repeated-media-payload")?;
    archive.finish()?;
    Ok(path)
}

fn write_pathological_html(output: &Path) -> Result<PathBuf> {
    let path = output.join("pathological.html");
    let mut file = std::fs::File::create(&path)?;
    file.write_all(b"<!doctype html><html><body>")?;
    for depth in 0..512 {
        write!(
            file,
            "<section data-depth=\"{depth}\" class=\"{}\"><!--{}-->",
            "repeated-class ".repeat(12),
            "comment-payload".repeat(8)
        )?;
    }
    for index in 0..8_000 {
        write!(file, "<span>bounded-{index}</span>")?;
    }
    for _ in 0..512 {
        file.write_all(b"</section>")?;
    }
    file.write_all(b"</body></html>")?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_fixture_bytes_are_deterministic() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        let first_manifest = generate(first.path()).unwrap();
        let second_manifest = generate(second.path()).unwrap();
        assert_eq!(first_manifest.len(), second_manifest.len());
        for (left, right) in first_manifest.iter().zip(&second_manifest) {
            assert_eq!(left.sha256, right.sha256);
            assert_eq!(left.raw_bytes, right.raw_bytes);
            assert_eq!(left.declared_bytes, right.declared_bytes);
        }
    }
}
