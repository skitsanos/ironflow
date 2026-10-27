use super::{
    super::{assets::Picture, config::Config},
    REL, WORD, Xml,
};
use crate::util::execution::ExecutionControl;
use anyhow::Result;
use std::collections::BTreeMap;

pub(super) fn build(
    config: &Config,
    pictures: &BTreeMap<String, Picture>,
    links: &BTreeMap<String, String>,
    execution: &ExecutionControl,
) -> Result<Vec<(&'static str, Vec<u8>)>> {
    Ok(vec![
        ("[Content_Types].xml", types(execution)?),
        (
            "_rels/.rels",
            relationships(None, pictures, links, execution)?,
        ),
        (
            "word/_rels/document.xml.rels",
            relationships(Some(()), pictures, links, execution)?,
        ),
        ("word/styles.xml", styles(config, execution)?),
        ("docProps/core.xml", metadata(config, execution)?),
    ])
}

fn types(execution: &ExecutionControl) -> Result<Vec<u8>> {
    let mut xml = Xml::new(execution)?;
    xml.start(
        "Types",
        &[(
            "xmlns",
            "http://schemas.openxmlformats.org/package/2006/content-types",
        )],
    )?;
    for (extension, mime) in [
        (
            "rels",
            "application/vnd.openxmlformats-package.relationships+xml",
        ),
        ("xml", "application/xml"),
        ("png", "image/png"),
    ] {
        xml.empty(
            "Default",
            &[("Extension", extension), ("ContentType", mime)],
        )?;
    }
    for (name, mime) in [
        (
            "/word/document.xml",
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml",
        ),
        (
            "/word/styles.xml",
            "application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml",
        ),
        (
            "/docProps/core.xml",
            "application/vnd.openxmlformats-package.core-properties+xml",
        ),
    ] {
        xml.empty("Override", &[("PartName", name), ("ContentType", mime)])?;
    }
    xml.end("Types")?;
    Ok(xml.finish())
}

fn relationships(
    document: Option<()>,
    pictures: &BTreeMap<String, Picture>,
    links: &BTreeMap<String, String>,
    execution: &ExecutionControl,
) -> Result<Vec<u8>> {
    let mut xml = Xml::new(execution)?;
    xml.start(
        "Relationships",
        &[(
            "xmlns",
            "http://schemas.openxmlformats.org/package/2006/relationships",
        )],
    )?;
    if document.is_some() {
        xml.empty(
            "Relationship",
            &[
                ("Id", "styles"),
                ("Type", &format!("{REL}/styles")),
                ("Target", "styles.xml"),
            ],
        )?;
        for (index, _) in pictures.values().enumerate() {
            xml.empty(
                "Relationship",
                &[
                    ("Id", &format!("image{}", index + 1)),
                    ("Type", &format!("{REL}/image")),
                    ("Target", &format!("media/image{}.png", index + 1)),
                ],
            )?;
        }
        for (url, id) in links {
            xml.empty(
                "Relationship",
                &[
                    ("Id", id),
                    ("Type", &format!("{REL}/hyperlink")),
                    ("Target", url),
                    ("TargetMode", "External"),
                ],
            )?;
        }
    } else {
        xml.empty(
            "Relationship",
            &[
                ("Id", "document"),
                ("Type", &format!("{REL}/officeDocument")),
                ("Target", "word/document.xml"),
            ],
        )?;
        xml.empty("Relationship", &[("Id", "core"), ("Type", "http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties"), ("Target", "docProps/core.xml")])?;
    }
    xml.end("Relationships")?;
    Ok(xml.finish())
}

fn metadata(config: &Config, execution: &ExecutionControl) -> Result<Vec<u8>> {
    let mut xml = Xml::new(execution)?;
    xml.start(
        "cp:coreProperties",
        &[
            (
                "xmlns:cp",
                "http://schemas.openxmlformats.org/package/2006/metadata/core-properties",
            ),
            ("xmlns:dc", "http://purl.org/dc/elements/1.1/"),
        ],
    )?;
    xml.element("dc:title", &config.title)?;
    xml.element("dc:creator", &config.author)?;
    xml.element("dc:language", &config.language)?;
    xml.end("cp:coreProperties")?;
    Ok(xml.finish())
}

fn styles(config: &Config, execution: &ExecutionControl) -> Result<Vec<u8>> {
    let mut xml = Xml::new(execution)?;
    xml.start("w:styles", &[("xmlns:w", WORD)])?;
    xml.start("w:docDefaults", &[])?;
    xml.start("w:rPrDefault", &[])?;
    xml.start("w:rPr", &[])?;
    xml.empty(
        "w:rFonts",
        &[("w:ascii", "Noto Serif"), ("w:hAnsi", "Noto Serif")],
    )?;
    xml.empty(
        "w:sz",
        &[("w:val", &(config.font_size * 2.0).round().to_string())],
    )?;
    xml.empty(
        "w:szCs",
        &[("w:val", &(config.font_size * 2.0).round().to_string())],
    )?;
    xml.end("w:rPr")?;
    xml.end("w:rPrDefault")?;
    xml.start("w:pPrDefault", &[])?;
    xml.start("w:pPr", &[])?;
    xml.empty(
        "w:spacing",
        &[
            ("w:after", "120"),
            ("w:line", "276"),
            ("w:lineRule", "auto"),
        ],
    )?;
    xml.end("w:pPr")?;
    xml.end("w:pPrDefault")?;
    xml.end("w:docDefaults")?;
    xml.start(
        "w:style",
        &[
            ("w:type", "paragraph"),
            ("w:styleId", "Normal"),
            ("w:default", "1"),
        ],
    )?;
    xml.empty("w:name", &[("w:val", "Normal")])?;
    xml.start("w:pPr", &[])?;
    xml.empty("w:bidi", &[("w:val", "0")])?;
    xml.empty("w:jc", &[("w:val", "left")])?;
    xml.end("w:pPr")?;
    xml.end("w:style")?;
    rtl_style(&mut xml, "Normal")?;
    for (index, scale) in [2.0, 1.5, 1.25, 1.15, 1.1, 1.1].iter().enumerate() {
        xml.start(
            "w:style",
            &[
                ("w:type", "paragraph"),
                ("w:styleId", &format!("Heading{}", index + 1)),
            ],
        )?;
        xml.empty("w:name", &[("w:val", &format!("heading {}", index + 1))])?;
        xml.start("w:pPr", &[])?;
        xml.empty("w:keepNext", &[])?;
        xml.empty("w:bidi", &[("w:val", "0")])?;
        xml.empty("w:spacing", &[("w:before", "240"), ("w:after", "120")])?;
        xml.empty("w:jc", &[("w:val", "left")])?;
        xml.empty("w:outlineLvl", &[("w:val", &index.to_string())])?;
        xml.end("w:pPr")?;
        xml.start("w:rPr", &[])?;
        xml.empty("w:b", &[])?;
        xml.empty("w:bCs", &[])?;
        xml.empty(
            "w:sz",
            &[(
                "w:val",
                &(config.font_size * scale * 2.0).round().to_string(),
            )],
        )?;
        xml.empty(
            "w:szCs",
            &[(
                "w:val",
                &(config.font_size * scale * 2.0).round().to_string(),
            )],
        )?;
        xml.end("w:rPr")?;
        xml.end("w:style")?;
        rtl_style(&mut xml, &format!("Heading{}", index + 1))?;
    }
    xml.end("w:styles")?;
    Ok(xml.finish())
}

fn rtl_style(xml: &mut Xml<'_>, base: &str) -> Result<()> {
    xml.start(
        "w:style",
        &[
            ("w:type", "paragraph"),
            ("w:styleId", &format!("{base}Rtl")),
        ],
    )?;
    xml.empty("w:name", &[("w:val", &format!("{base} RTL"))])?;
    xml.empty("w:basedOn", &[("w:val", base)])?;
    xml.start("w:pPr", &[])?;
    xml.empty("w:bidi", &[("w:val", "1")])?;
    xml.empty("w:jc", &[("w:val", "right")])?;
    xml.end("w:pPr")?;
    xml.end("w:style")
}
