use std::io::Read;

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
    /// Resolved hex color (uppercase, no leading '#'), e.g. "0066FF". None if not set, "auto", or unresolved.
    pub(super) color: Option<String>,
    /// Highlight (background) color name as defined in OOXML, e.g. "yellow".
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

/// Parse word/theme/theme1.xml and return a map from themeColor name → hex string.
/// Theme color names align with OOXML w:themeColor values: "dark1", "light1", "dark2", "light2",
/// "accent1"..."accent6", "hyperlink", "followedHyperlink". Returns an empty map if the theme file
/// is absent or malformed.
pub(super) fn parse_theme_colors(
    archive: &mut zip::ZipArchive<std::fs::File>,
) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    let xml = match archive.by_name("word/theme/theme1.xml") {
        Ok(mut entry) => {
            let mut buf = String::new();
            if entry.read_to_string(&mut buf).is_ok() {
                buf
            } else {
                return map;
            }
        }
        Err(_) => return map,
    };

    // Maps clrScheme child element names to themeColor canonical names used in document.xml.
    // OOXML uses dk1/lt1/dk2/lt2 in theme1.xml; w:themeColor uses dark1/light1/dark2/light2.
    let translate = |local: &str| -> Option<&'static str> {
        match local {
            "dk1" => Some("dark1"),
            "lt1" => Some("light1"),
            "dk2" => Some("dark2"),
            "lt2" => Some("light2"),
            "accent1" => Some("accent1"),
            "accent2" => Some("accent2"),
            "accent3" => Some("accent3"),
            "accent4" => Some("accent4"),
            "accent5" => Some("accent5"),
            "accent6" => Some("accent6"),
            "hlink" => Some("hyperlink"),
            "folHlink" => Some("followedHyperlink"),
            _ => None,
        }
    };

    use quick_xml::events::Event;
    let mut reader = quick_xml::Reader::from_str(&xml);
    let mut buf = Vec::new();

    let mut in_clr_scheme = false;
    let mut current_role: Option<&'static str> = None;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e) | Event::Empty(ref e)) => {
                let raw = String::from_utf8_lossy(e.name().as_ref()).to_string();
                let local = raw.rsplit(':').next().unwrap_or(&raw);

                if local == "clrScheme" {
                    in_clr_scheme = true;
                } else if in_clr_scheme {
                    if let Some(role) = translate(local) {
                        current_role = Some(role);
                    } else if (local == "srgbClr" || local == "sysClr")
                        && let Some(role) = current_role
                    {
                        let attr_name: &[u8] = if local == "srgbClr" {
                            b"val"
                        } else {
                            b"lastClr"
                        };
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == attr_name {
                                let v = String::from_utf8_lossy(&attr.value).to_uppercase();
                                map.insert(role.to_string(), v);
                            }
                        }
                    }
                }
            }
            Ok(Event::End(ref e)) => {
                let raw = String::from_utf8_lossy(e.name().as_ref()).to_string();
                let local = raw.rsplit(':').next().unwrap_or(&raw);
                if local == "clrScheme" {
                    in_clr_scheme = false;
                    current_role = None;
                } else if translate(local).is_some() {
                    current_role = None;
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    map
}

/// Parse numbering.xml to identify which numId values are numbered vs bulleted.
pub(super) fn parse_numbering_defs(
    archive: &mut zip::ZipArchive<std::fs::File>,
) -> std::collections::HashMap<String, bool> {
    // Maps numId -> is_numbered (true = ordered, false = bullet)
    let mut result = std::collections::HashMap::new();

    let xml = match archive.by_name("word/numbering.xml") {
        Ok(mut entry) => {
            let mut buf = String::new();
            if entry.read_to_string(&mut buf).is_ok() {
                buf
            } else {
                return result;
            }
        }
        Err(_) => return result,
    };

    use quick_xml::events::Event;
    let mut reader = quick_xml::Reader::from_str(&xml);
    let mut buf = Vec::new();

    // Track abstractNum definitions: abstractNumId -> is_numbered
    let mut abstract_defs: std::collections::HashMap<String, bool> =
        std::collections::HashMap::new();
    let mut current_abstract_id: Option<String> = None;

    // Track num -> abstractNumId mapping
    let mut num_to_abstract: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    let mut current_num_id: Option<String> = None;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e) | Event::Empty(ref e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                match name.as_str() {
                    "w:abstractNum" => {
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"w:abstractNumId" {
                                current_abstract_id =
                                    Some(String::from_utf8_lossy(&attr.value).to_string());
                            }
                        }
                    }
                    "w:lvl" => {}
                    "w:numFmt" => {
                        if let Some(ref id) = current_abstract_id {
                            for attr in e.attributes().flatten() {
                                if attr.key.as_ref() == b"w:val" {
                                    let val = String::from_utf8_lossy(&attr.value).to_string();
                                    let is_numbered = val != "bullet" && val != "none";
                                    abstract_defs.insert(id.clone(), is_numbered);
                                }
                            }
                        }
                    }
                    "w:num" => {
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"w:numId" {
                                current_num_id =
                                    Some(String::from_utf8_lossy(&attr.value).to_string());
                            }
                        }
                    }
                    "w:abstractNumId" => {
                        if let Some(ref num_id) = current_num_id {
                            for attr in e.attributes().flatten() {
                                if attr.key.as_ref() == b"w:val" {
                                    num_to_abstract.insert(
                                        num_id.clone(),
                                        String::from_utf8_lossy(&attr.value).to_string(),
                                    );
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::End(ref e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if name == "w:abstractNum" {
                    current_abstract_id = None;
                } else if name == "w:num" {
                    current_num_id = None;
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    // Resolve: numId -> is_numbered
    for (num_id, abstract_id) in &num_to_abstract {
        if let Some(&is_numbered) = abstract_defs.get(abstract_id) {
            result.insert(num_id.clone(), is_numbered);
        }
    }

    result
}

/// Walk word/document.xml and emit blocks (paragraphs + tables) in document order.
/// Captures run-level bold/italic/underline/strike, color (with theme resolution), highlight,
/// and paragraph style / list state. Tables nested inside cells are flattened — their paragraphs
/// become direct cell paragraphs and no nested table block is emitted.
pub(super) fn parse_docx_blocks(
    xml: &str,
    numbering_defs: &std::collections::HashMap<String, bool>,
    theme_colors: &std::collections::HashMap<String, String>,
) -> Vec<DocxBlock> {
    use quick_xml::events::Event;

    let mut reader = quick_xml::Reader::from_str(xml);
    let mut buf = Vec::new();
    let mut blocks: Vec<DocxBlock> = Vec::new();

    // Element-state flags
    let mut in_paragraph = false;
    let mut in_run = false;
    let mut in_run_props = false;
    let mut in_para_props = false;

    // Table state — table_depth tracks nesting depth (0 = not in any table, 1 = top-level table,
    // 2+ = nested). Open tables and rows are stacked so we can emit them on close.
    let mut table_stack: Vec<DocxTable> = Vec::new();
    let mut row_stack: Vec<DocxRow> = Vec::new();
    // Current cell paragraphs (the innermost cell currently being filled).
    let mut cell_stack: Vec<DocxCell> = Vec::new();

    let mut current_para = DocxParagraph::default();
    let mut current_run = DocxRun::default();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e) | Event::Empty(ref e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();

                match name.as_str() {
                    // ---- Tables ----
                    "w:tbl" => {
                        table_stack.push(DocxTable::default());
                    }
                    "w:tr" if !table_stack.is_empty() => {
                        row_stack.push(DocxRow::default());
                    }
                    "w:tc" if !row_stack.is_empty() => {
                        cell_stack.push(DocxCell::default());
                    }

                    // ---- Paragraph open ----
                    "w:p" => {
                        in_paragraph = true;
                        current_para = DocxParagraph::default();
                    }
                    "w:pPr" if in_paragraph => {
                        in_para_props = true;
                    }
                    "w:pStyle" if in_para_props => {
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"w:val" {
                                current_para.style =
                                    Some(String::from_utf8_lossy(&attr.value).to_string());
                            }
                        }
                    }
                    "w:numPr" if in_para_props => {
                        current_para.is_list_item = true;
                    }
                    "w:ilvl" if in_para_props => {
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"w:val"
                                && let Ok(level) =
                                    String::from_utf8_lossy(&attr.value).parse::<u32>()
                            {
                                current_para.list_level = level;
                            }
                        }
                    }
                    "w:numId" if in_para_props => {
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"w:val" {
                                let num_id = String::from_utf8_lossy(&attr.value).to_string();
                                current_para.is_numbered =
                                    numbering_defs.get(&num_id).copied().unwrap_or(false);
                            }
                        }
                    }

                    // ---- Run open ----
                    "w:r" if in_paragraph => {
                        in_run = true;
                        current_run = DocxRun::default();
                    }
                    "w:rPr" if in_run => {
                        in_run_props = true;
                    }
                    "w:b" if in_run_props => {
                        current_run.bold = true;
                    }
                    "w:i" if in_run_props => {
                        current_run.italic = true;
                    }
                    "w:u" if in_run_props => {
                        current_run.underline = true;
                    }
                    "w:strike" if in_run_props => {
                        current_run.strikethrough = true;
                    }
                    "w:color" if in_run_props => {
                        let mut hex: Option<String> = None;
                        let mut theme: Option<String> = None;
                        for attr in e.attributes().flatten() {
                            match attr.key.as_ref() {
                                b"w:val" => {
                                    let v = String::from_utf8_lossy(&attr.value).to_string();
                                    if v != "auto" && !v.is_empty() {
                                        hex = Some(v.to_uppercase());
                                    }
                                }
                                b"w:themeColor" => {
                                    theme = Some(String::from_utf8_lossy(&attr.value).to_string());
                                }
                                _ => {}
                            }
                        }
                        if let Some(h) = hex {
                            current_run.color = Some(h);
                        } else if let Some(t) = theme
                            && let Some(resolved) = theme_colors.get(&t)
                        {
                            current_run.color = Some(resolved.clone());
                        }
                    }
                    "w:highlight" if in_run_props => {
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"w:val" {
                                let v = String::from_utf8_lossy(&attr.value).to_string();
                                if v != "none" && !v.is_empty() {
                                    current_run.highlight = Some(v);
                                }
                            }
                        }
                    }
                    "w:tab" if in_run => {
                        current_run.text.push('\t');
                    }
                    "w:br" if in_run => {
                        current_run.text.push('\n');
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(ref e)) if in_run => {
                let text = String::from_utf8_lossy(e.as_ref());
                current_run.text.push_str(&text);
            }
            Ok(Event::End(ref e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                match name.as_str() {
                    "w:p" => {
                        in_paragraph = false;
                        let finished = std::mem::take(&mut current_para);
                        // Decide where this paragraph belongs:
                        // - Inside a cell → push into innermost cell.paragraphs
                        // - Otherwise → top-level block list.
                        if let Some(cell) = cell_stack.last_mut() {
                            cell.paragraphs.push(finished);
                        } else {
                            blocks.push(DocxBlock::Paragraph(finished));
                        }
                    }
                    "w:r" => {
                        in_run = false;
                        if !current_run.text.is_empty() {
                            current_para.runs.push(std::mem::take(&mut current_run));
                        } else {
                            current_run = DocxRun::default();
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
                    "w:tbl" => {
                        if let Some(table) = table_stack.pop() {
                            if table_stack.is_empty() {
                                // Top-level table → emit as block
                                blocks.push(DocxBlock::Table(table));
                            } else {
                                // Nested table → flatten: append each cell's paragraphs into
                                // the surrounding cell's paragraph list, in row-major order.
                                if let Some(parent_cell) = cell_stack.last_mut() {
                                    for row in table.rows {
                                        for mut cell in row.cells {
                                            parent_cell.paragraphs.append(&mut cell.paragraphs);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    blocks
}
