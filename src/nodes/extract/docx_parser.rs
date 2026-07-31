mod anchors;
mod numbering;
mod theme;
mod xml;

use std::io::BufRead;

use anyhow::Result;

use anchors::AnchorCollector;
pub(super) use numbering::parse_numbering_defs;
pub(super) use theme::parse_theme_colors;
pub(super) use xml::{XmlDocument, visit_attributes};

use super::resource::Budget;

/// Structured representation of a DOCX paragraph.
#[derive(Default, Clone)]
pub(super) struct DocxParagraph {
    pub(super) style: Option<String>,
    pub(super) runs: Vec<DocxRun>,
    pub(super) is_list_item: bool,
    pub(super) list_level: u32,
    pub(super) is_numbered: bool,
}

#[derive(Default, Clone)]
pub(super) struct DocxRun {
    pub(super) text: String,
    pub(super) bold: bool,
    pub(super) italic: bool,
    pub(super) underline: bool,
    pub(super) strikethrough: bool,
    /// Resolved uppercase hex color without a leading `#`.
    pub(super) color: Option<String>,
    /// OOXML highlight color name, for example `yellow`.
    pub(super) highlight: Option<String>,
}

#[derive(Default, Clone)]
pub(super) struct DocxTable {
    pub(super) rows: Vec<DocxRow>,
}

#[derive(Default, Clone)]
pub(super) struct DocxRow {
    pub(super) cells: Vec<DocxCell>,
}

#[derive(Default, Clone)]
pub(super) struct DocxCell {
    pub(super) paragraphs: Vec<DocxParagraph>,
}

#[derive(Clone)]
pub(super) enum DocxBlock {
    Paragraph(DocxParagraph),
    Table(DocxTable),
}

pub(super) struct ParsedDocxDocument {
    pub(super) blocks: Vec<DocxBlock>,
    pub(super) anchors: std::collections::HashMap<String, String>,
}

/// Walk `word/document.xml` and emit paragraphs and tables in document order.
pub(super) fn parse_docx_blocks<R: BufRead>(
    xml: R,
    numbering_defs: &std::collections::HashMap<String, bool>,
    theme_colors: &std::collections::HashMap<String, String>,
    comment_ids: Option<&std::collections::HashSet<&str>>,
    budget: &mut Budget<'_>,
) -> Result<ParsedDocxDocument> {
    use quick_xml::events::Event;

    let mut reader = quick_xml::Reader::from_reader(xml);
    reader.config_mut().check_comments = true;
    let mut buf = Vec::new();
    let mut blocks = Vec::new();
    let mut in_paragraph = false;
    let mut in_run = false;
    let mut in_run_props = false;
    let mut in_para_props = false;
    let mut table_stack = Vec::<DocxTable>::new();
    let mut row_stack = Vec::<DocxRow>::new();
    let mut cell_stack = Vec::<DocxCell>::new();
    let mut current_para = DocxParagraph::default();
    let mut current_run = DocxRun::default();
    let mut document = XmlDocument::new("word/document.xml");
    let mut anchors = comment_ids.map(AnchorCollector::new);

    loop {
        budget.charge_item("DOCX XML events")?;
        let event = reader
            .read_event_into(&mut buf)
            .map_err(|error| anyhow::anyhow!("extract_word: invalid word/document.xml: {error}"))?;
        document.observe(&event, budget)?;
        if let Some(anchors) = anchors.as_mut() {
            anchors.observe(&event, budget)?;
        }
        match event {
            Event::Start(ref event) | Event::Empty(ref event) => {
                let name = String::from_utf8_lossy(event.name().as_ref()).to_string();
                match name.as_str() {
                    "w:tbl" => table_stack.push(DocxTable::default()),
                    "w:tr" if !table_stack.is_empty() => {
                        row_stack.push(DocxRow::default());
                    }
                    "w:tc" if !row_stack.is_empty() => {
                        cell_stack.push(DocxCell::default());
                    }
                    "w:p" => {
                        in_paragraph = true;
                        current_para = DocxParagraph::default();
                    }
                    "w:pPr" if in_paragraph => in_para_props = true,
                    "w:pStyle" if in_para_props => {
                        visit_attributes(event, "word/document.xml", budget, |key, value, _| {
                            if key == b"w:val" {
                                current_para.style =
                                    Some(String::from_utf8_lossy(value).to_string());
                            }
                            Ok(())
                        })?;
                    }
                    "w:numPr" if in_para_props => current_para.is_list_item = true,
                    "w:ilvl" if in_para_props => {
                        visit_attributes(
                            event,
                            "word/document.xml",
                            budget,
                            |key, value, budget| {
                                if key == b"w:val"
                                    && let Ok(level) = String::from_utf8_lossy(value).parse::<u32>()
                                {
                                    budget.charge_items(
                                        u64::from(level),
                                        "DOCX list indentation work",
                                    )?;
                                    current_para.list_level = level;
                                }
                                Ok(())
                            },
                        )?;
                    }
                    "w:numId" if in_para_props => {
                        visit_attributes(event, "word/document.xml", budget, |key, value, _| {
                            if key == b"w:val" {
                                current_para.is_numbered = numbering_defs
                                    .get(String::from_utf8_lossy(value).as_ref())
                                    .copied()
                                    .unwrap_or(false);
                            }
                            Ok(())
                        })?;
                    }
                    "w:r" if in_paragraph => {
                        in_run = true;
                        current_run = DocxRun::default();
                    }
                    "w:rPr" if in_run => in_run_props = true,
                    "w:b" if in_run_props => current_run.bold = true,
                    "w:i" if in_run_props => current_run.italic = true,
                    "w:u" if in_run_props => current_run.underline = true,
                    "w:strike" if in_run_props => current_run.strikethrough = true,
                    "w:color" if in_run_props => {
                        apply_run_color(event, &mut current_run, theme_colors, budget)?;
                    }
                    "w:highlight" if in_run_props => {
                        visit_attributes(event, "word/document.xml", budget, |key, value, _| {
                            if key == b"w:val" {
                                let value = String::from_utf8_lossy(value).to_string();
                                if value != "none" && !value.is_empty() {
                                    current_run.highlight = Some(value);
                                }
                            }
                            Ok(())
                        })?;
                    }
                    "w:tab" if in_run => {
                        budget.charge_output(1, "DOCX extracted text")?;
                        current_run.text.push('\t');
                    }
                    "w:br" if in_run => {
                        budget.charge_output(1, "DOCX extracted text")?;
                        current_run.text.push('\n');
                    }
                    _ => {}
                }
            }
            Event::Text(ref event) if in_run => {
                budget.charge_output(event.len() as u64, "DOCX extracted text")?;
                current_run
                    .text
                    .push_str(&String::from_utf8_lossy(event.as_ref()));
            }
            Event::End(ref event) => {
                let name = String::from_utf8_lossy(event.name().as_ref()).to_string();
                match name.as_str() {
                    "w:p" => {
                        in_paragraph = false;
                        let finished = std::mem::take(&mut current_para);
                        if let Some(cell) = cell_stack.last_mut() {
                            cell.paragraphs.push(finished);
                        } else {
                            blocks.push(DocxBlock::Paragraph(finished));
                        }
                    }
                    "w:r" => {
                        in_run = false;
                        if current_run.text.is_empty() {
                            current_run = DocxRun::default();
                        } else {
                            current_para.runs.push(std::mem::take(&mut current_run));
                        }
                    }
                    "w:rPr" => in_run_props = false,
                    "w:pPr" => in_para_props = false,
                    "w:tc" => {
                        if let Some(cell) = cell_stack.pop()
                            && let Some(row) = row_stack.last_mut()
                        {
                            row.cells.push(cell);
                        }
                    }
                    "w:tr" => {
                        if let Some(row) = row_stack.pop()
                            && let Some(table) = table_stack.last_mut()
                        {
                            table.rows.push(row);
                        }
                    }
                    "w:tbl" => finish_table(&mut table_stack, &mut cell_stack, &mut blocks),
                    _ => {}
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }

    Ok(ParsedDocxDocument {
        blocks,
        anchors: anchors.map(AnchorCollector::finish).unwrap_or_default(),
    })
}

fn apply_run_color(
    event: &quick_xml::events::BytesStart<'_>,
    run: &mut DocxRun,
    theme_colors: &std::collections::HashMap<String, String>,
    budget: &mut Budget<'_>,
) -> Result<()> {
    let mut hex = None;
    let mut theme = None;
    visit_attributes(event, "word/document.xml", budget, |key, value, _| {
        match key {
            b"w:val" => {
                let value = String::from_utf8_lossy(value).to_string();
                if value != "auto" && !value.is_empty() {
                    hex = Some(value.to_uppercase());
                }
            }
            b"w:themeColor" => {
                theme = Some(String::from_utf8_lossy(value).to_string());
            }
            _ => {}
        }
        Ok(())
    })?;
    run.color = hex.or_else(|| theme.and_then(|key| theme_colors.get(&key).cloned()));
    Ok(())
}

fn finish_table(
    table_stack: &mut Vec<DocxTable>,
    cell_stack: &mut [DocxCell],
    blocks: &mut Vec<DocxBlock>,
) {
    let Some(table) = table_stack.pop() else {
        return;
    };
    if table_stack.is_empty() {
        blocks.push(DocxBlock::Table(table));
    } else if let Some(parent_cell) = cell_stack.last_mut() {
        for row in table.rows {
            for mut cell in row.cells {
                parent_cell.paragraphs.append(&mut cell.paragraphs);
            }
        }
    }
}
