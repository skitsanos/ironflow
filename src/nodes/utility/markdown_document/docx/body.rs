use super::{
    super::{
        assets::Picture,
        model::{Block, Run, Style},
    },
    Xml, drawing, text,
};
use anyhow::Result;
use std::collections::BTreeMap;

pub(super) fn write(
    xml: &mut Xml<'_>,
    blocks: &[Block],
    pictures: &BTreeMap<String, Picture>,
    links: &BTreeMap<String, String>,
    config: &super::super::config::Config,
) -> Result<()> {
    if blocks.is_empty() {
        xml.empty("w:p", &[])?;
    }
    for (index, block) in blocks.iter().enumerate() {
        match block {
            Block::Paragraph { runs, depth, label } => {
                let mut content = runs.clone();
                if !label.is_empty() {
                    content.insert(
                        0,
                        Run {
                            text: label.clone(),
                            style: Style::default(),
                        },
                    );
                }
                xml.start("w:p", &[])?;
                xml.start("w:pPr", &[])?;
                text::direction_props(xml, &content, config, *depth, "Normal")?;
                xml.end("w:pPr")?;
                text::runs(xml, &content, links, config)?;
                xml.end("w:p")?;
            }
            Block::Heading(level, runs) => {
                xml.start("w:p", &[])?;
                xml.start("w:pPr", &[])?;
                text::direction_props(xml, runs, config, 0, &format!("Heading{level}"))?;
                xml.end("w:pPr")?;
                text::runs(xml, runs, links, config)?;
                xml.end("w:p")?;
            }
            Block::Code(text) => {
                xml.start("w:p", &[])?;
                xml.start("w:pPr", &[])?;
                xml.empty("w:pStyle", &[("w:val", "Normal")])?;
                xml.empty("w:bidi", &[("w:val", "0")])?;
                xml.empty("w:jc", &[("w:val", "left")])?;
                xml.end("w:pPr")?;
                text::runs(
                    xml,
                    &[Run {
                        text: text.trim_end_matches('\n').to_owned(),
                        style: Style {
                            code: true,
                            ..Default::default()
                        },
                    }],
                    links,
                    config,
                )?;
                xml.end("w:p")?;
            }
            Block::Table(rows) => table(xml, rows, links, config)?,
            Block::Image { key, alt } => {
                let (image_index, _) = pictures
                    .keys()
                    .enumerate()
                    .find(|(_, k)| *k == key)
                    .ok_or_else(|| anyhow::anyhow!("missing picture binding"))?;
                drawing::write(xml, &pictures[key], image_index + 1, index + 1, alt)?;
            }
            Block::Rule => {
                xml.start("w:p", &[])?;
                xml.start("w:pPr", &[])?;
                xml.start("w:pBdr", &[])?;
                xml.empty(
                    "w:bottom",
                    &[("w:val", "single"), ("w:sz", "6"), ("w:color", "999999")],
                )?;
                xml.end("w:pBdr")?;
                xml.end("w:pPr")?;
                xml.end("w:p")?;
            }
        }
    }
    Ok(())
}

fn table(
    xml: &mut Xml<'_>,
    rows: &[Vec<Vec<Run>>],
    links: &BTreeMap<String, String>,
    config: &super::super::config::Config,
) -> Result<()> {
    let columns = rows.iter().map(Vec::len).max().unwrap_or(1).max(1);
    let width = ((config.width - 2.0 * config.margin) * 20.0).round() as usize;
    let widths: Vec<String> = (0..columns)
        .map(|column| ((column + 1) * width / columns - column * width / columns).to_string())
        .collect();
    xml.start("w:tbl", &[])?;
    xml.start("w:tblPr", &[])?;
    let first = rows
        .first()
        .map(|row| {
            row.iter()
                .flatten()
                .map(|run| run.text.as_str())
                .collect::<String>()
        })
        .unwrap_or_default();
    if config.direction.rtl(&first) {
        xml.empty("w:bidiVisual", &[])?;
    }
    xml.empty("w:tblW", &[("w:w", &width.to_string()), ("w:type", "dxa")])?;
    xml.empty("w:tblLayout", &[("w:type", "fixed")])?;
    xml.start("w:tblBorders", &[])?;
    for edge in ["top", "left", "bottom", "right", "insideH", "insideV"] {
        xml.empty(
            &format!("w:{edge}"),
            &[("w:val", "single"), ("w:sz", "4"), ("w:color", "CCCCCC")],
        )?;
    }
    xml.end("w:tblBorders")?;
    xml.end("w:tblPr")?;
    xml.start("w:tblGrid", &[])?;
    for width in &widths {
        xml.empty("w:gridCol", &[("w:w", width)])?;
    }
    xml.end("w:tblGrid")?;
    for (index, row) in rows.iter().enumerate() {
        xml.start("w:tr", &[])?;
        if index == 0 {
            xml.start("w:trPr", &[])?;
            xml.empty("w:tblHeader", &[])?;
            xml.end("w:trPr")?;
        }
        for (column, cell) in row.iter().enumerate() {
            xml.start("w:tc", &[])?;
            xml.start("w:tcPr", &[])?;
            xml.empty("w:tcW", &[("w:w", &widths[column]), ("w:type", "dxa")])?;
            xml.end("w:tcPr")?;
            xml.start("w:p", &[])?;
            xml.start("w:pPr", &[])?;
            text::direction_props(xml, cell, config, 0, "Normal")?;
            xml.end("w:pPr")?;
            text::runs(xml, cell, links, config)?;
            xml.end("w:p")?;
            xml.end("w:tc")?;
        }
        xml.end("w:tr")?;
    }
    xml.end("w:tbl")?;
    // A paragraph following a table is required by Word, including at EOF.
    xml.empty("w:p", &[])
}
