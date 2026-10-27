use super::{assets::Picture, config::Config, model::Block, output::CappedWriter};
use crate::util::execution::ExecutionControl;
use anyhow::Result;
use std::{collections::BTreeMap, io::Write};
use xml::Xml;
use zip::{ZipWriter, write::SimpleFileOptions};

mod body;
mod drawing;
mod parts;
mod text;
mod xml;

const WORD: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

pub(super) fn render(
    writer: CappedWriter<'_>,
    config: &Config,
    blocks: &[Block],
    pictures: &BTreeMap<String, Picture>,
    execution: &ExecutionControl,
) -> Result<()> {
    let links = links(blocks);
    let mut archive = ZipWriter::new(writer);
    let mut xml = Xml::new(execution)?;
    xml.start(
        "w:document",
        &[
            ("xmlns:w", WORD),
            ("xmlns:r", REL),
            (
                "xmlns:mc",
                "http://schemas.openxmlformats.org/markup-compatibility/2006",
            ),
            (
                "xmlns:w14",
                "http://schemas.microsoft.com/office/word/2010/wordml",
            ),
            ("mc:Ignorable", "w14"),
        ],
    )?;
    xml.start("w:body", &[])?;
    body::write(&mut xml, blocks, pictures, &links, config)?;
    xml.start("w:sectPr", &[])?;
    xml.empty(
        "w:pgSz",
        &[
            ("w:w", &twips(config.width)),
            ("w:h", &twips(config.height)),
        ],
    )?;
    let margin = twips(config.margin);
    xml.empty(
        "w:pgMar",
        &[
            ("w:top", &margin),
            ("w:right", &margin),
            ("w:bottom", &margin),
            ("w:left", &margin),
            ("w:header", "0"),
            ("w:footer", "0"),
            ("w:gutter", "0"),
        ],
    )?;
    xml.end("w:sectPr")?;
    xml.end("w:body")?;
    xml.end("w:document")?;
    add(&mut archive, "word/document.xml", &xml.finish())?;
    for (name, bytes) in parts::build(config, pictures, &links, execution)? {
        add(&mut archive, name, &bytes)?;
    }
    for (index, picture) in pictures.values().enumerate() {
        execution.checkpoint()?;
        add(
            &mut archive,
            &format!("word/media/image{}.png", index + 1),
            &picture.png,
        )?;
    }
    archive.finish()?.flush()?;
    Ok(())
}

fn add(archive: &mut ZipWriter<CappedWriter<'_>>, name: &str, bytes: &[u8]) -> Result<()> {
    archive.start_file(
        name,
        SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated),
    )?;
    archive.write_all(bytes)?;
    Ok(())
}

fn twips(points: f32) -> String {
    (points * 20.0).round().to_string()
}

fn links(blocks: &[Block]) -> BTreeMap<String, String> {
    let mut result = BTreeMap::new();
    let mut add = |runs: &[super::model::Run]| {
        for run in runs {
            if let Some(url) = &run.style.link {
                let id = format!("link{}", result.len() + 1);
                if !result.contains_key(url.as_ref()) {
                    result.insert(url.to_string(), id);
                }
            }
        }
    };
    for block in blocks {
        match block {
            Block::Paragraph { runs, .. } | Block::Heading(_, runs) => add(runs),
            Block::Table(rows) => {
                for cell in rows.iter().flatten() {
                    add(cell);
                }
            }
            _ => {}
        }
    }
    result
}
