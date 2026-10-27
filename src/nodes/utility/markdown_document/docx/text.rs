use super::{
    super::{
        config::Config,
        language::{Direction, Face},
        model::{Run, Style},
    },
    Xml,
};
use anyhow::Result;
use std::collections::BTreeMap;
use unicode_bidi::BidiInfo;

pub(super) fn direction_props(
    xml: &mut Xml<'_>,
    runs: &[Run],
    config: &Config,
    depth: usize,
    style: &str,
) -> Result<()> {
    let text: String = runs.iter().map(|run| run.text.as_str()).collect();
    let rtl = config.direction.rtl(&text);
    let style = if rtl {
        format!("{style}Rtl")
    } else {
        style.to_owned()
    };
    xml.empty("w:pStyle", &[("w:val", &style)])?;
    xml.empty("w:bidi", &[("w:val", if rtl { "1" } else { "0" })])?;
    if depth > 0 {
        xml.empty(
            "w:ind",
            &[(
                if rtl { "w:right" } else { "w:left" },
                &(depth * 240).min(2400).to_string(),
            )],
        )?;
    }
    if !rtl {
        return xml.empty("w:jc", &[("w:val", "left")]);
    }
    // Older importers use physical alignment; Word/LibreOffice mirror it in RTL.
    // A physical style also covers previewers that skip AlternateContent entirely.
    xml.start("mc:AlternateContent", &[])?;
    xml.start("mc:Choice", &[("Requires", "w14")])?;
    xml.empty("w:jc", &[("w:val", "start")])?;
    xml.end("mc:Choice")?;
    xml.start("mc:Fallback", &[])?;
    xml.empty("w:jc", &[("w:val", "right")])?;
    xml.end("mc:Fallback")?;
    xml.end("mc:AlternateContent")
}

pub(super) fn runs(
    xml: &mut Xml<'_>,
    runs: &[Run],
    links: &BTreeMap<String, String>,
    config: &Config,
) -> Result<()> {
    let text: String = runs.iter().map(|run| run.text.as_str()).collect();
    let direction = if runs.iter().all(|r| r.style.code) {
        Direction::Ltr
    } else {
        config.direction
    };
    let bidi = BidiInfo::new(&text, direction.level());
    let cjk_language = Face::Cjk.locale(&text, &config.language);
    let locale = |face: Face| {
        if face == Face::Cjk {
            cjk_language
        } else {
            face.locale("", &config.language)
        }
    };
    let mut offset = 0;
    for run in runs {
        if let Some(url) = &run.style.link {
            xml.start("w:hyperlink", &[("r:id", &links[url.as_ref()])])?;
        }
        let mut face = run.text.chars().find_map(Face::of).unwrap_or(Face::Latin);
        let mut rtl = bidi.levels.get(offset).is_some_and(|level| level.is_rtl());
        let mut start = 0;
        for (index, c) in run.text.char_indices() {
            let next_face = Face::of(c).unwrap_or(face);
            let next_rtl = bidi.levels[offset + index].is_rtl();
            if next_face != face || next_rtl != rtl {
                fragment(
                    xml,
                    &run.text[start..index],
                    &run.style,
                    face,
                    rtl,
                    locale(face),
                )?;
                start = index;
                face = next_face;
                rtl = next_rtl;
            }
        }
        fragment(xml, &run.text[start..], &run.style, face, rtl, locale(face))?;
        offset += run.text.len();
        if run.style.link.is_some() {
            xml.end("w:hyperlink")?;
        }
    }
    Ok(())
}

fn fragment(
    xml: &mut Xml<'_>,
    text: &str,
    style: &Style,
    face: Face,
    rtl: bool,
    language: &str,
) -> Result<()> {
    if text.is_empty() {
        return Ok(());
    }
    let family = if style.code && face == Face::Latin {
        "Noto Sans Mono"
    } else {
        face.family()
    };
    xml.start("w:r", &[])?;
    xml.start("w:rPr", &[])?;
    xml.empty(
        "w:rFonts",
        &[
            ("w:ascii", family),
            ("w:hAnsi", family),
            ("w:eastAsia", Face::Cjk.family()),
            ("w:cs", family),
        ],
    )?;
    if style.bold {
        xml.empty("w:b", &[])?;
        xml.empty("w:bCs", &[])?;
    }
    if style.italic {
        xml.empty("w:i", &[])?;
        xml.empty("w:iCs", &[])?;
    }
    if style.strike {
        xml.empty("w:strike", &[])?;
    }
    if style.link.is_some() {
        xml.empty("w:color", &[("w:val", "0563C1")])?;
        xml.empty("w:u", &[("w:val", "single")])?;
    }
    xml.empty("w:rtl", &[("w:val", if rtl { "1" } else { "0" })])?;
    if language != "und" {
        xml.empty(
            "w:lang",
            &[
                ("w:val", language),
                ("w:eastAsia", language),
                ("w:bidi", language),
            ],
        )?;
    }
    xml.end("w:rPr")?;
    for (index, line) in text.split('\n').enumerate() {
        if index > 0 {
            xml.empty("w:br", &[])?;
        }
        for (index, text) in line.split('\t').enumerate() {
            if index > 0 {
                xml.empty("w:tab", &[])?;
            }
            xml.start("w:t", &[("xml:space", "preserve")])?;
            xml.text(text)?;
            xml.end("w:t")?;
        }
    }
    xml.end("w:r")
}
