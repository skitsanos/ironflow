use super::{PptxElement, PptxTextPara};

/// Parse a single slide XML document into its title and ordered elements.
pub(in crate::nodes::extract) fn parse_pptx_slide(xml: &str) -> (Option<String>, Vec<PptxElement>) {
    use quick_xml::events::Event;

    let mut reader = quick_xml::Reader::from_str(xml);
    let mut buf = Vec::new();
    let mut title = None;
    let mut elements = Vec::new();
    let mut placeholder = None;
    let mut in_tx_body = false;
    let mut in_para = false;
    let mut current_text = String::new();
    let mut current_list_level = None;
    let mut current_paras = Vec::new();
    let mut in_run = false;
    let mut in_text = false;
    let mut in_table = false;
    let mut table_rows = Vec::new();
    let mut current_row = Vec::new();
    let mut current_cell_text = String::new();
    let mut in_cell = false;
    let mut in_picture = false;
    let mut picture_alt = None;
    let mut picture_embed_id = None;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref event) | Event::Empty(ref event)) => {
                let raw = String::from_utf8_lossy(event.name().as_ref()).to_string();
                let local = raw.rsplit(':').next().unwrap_or(&raw);
                match local {
                    "sp" => {
                        placeholder = None;
                        current_paras.clear();
                    }
                    "ph" => collect_placeholder(event, &mut placeholder),
                    "txBody" => in_tx_body = true,
                    "p" if in_tx_body => {
                        in_para = true;
                        current_text.clear();
                        current_list_level = None;
                    }
                    "r" if in_para => in_run = true,
                    "t" if in_run => in_text = true,
                    "tbl" => {
                        in_table = true;
                        table_rows.clear();
                    }
                    "tr" if in_table => current_row.clear(),
                    "tc" if in_table => {
                        in_cell = true;
                        current_cell_text.clear();
                    }
                    "pic" => {
                        in_picture = true;
                        picture_alt = None;
                        picture_embed_id = None;
                    }
                    "cNvPr" if in_picture => collect_picture_alt(event, &mut picture_alt),
                    "blip" if in_picture => {
                        collect_picture_embed_id(event, &mut picture_embed_id);
                    }
                    _ => {}
                }
                if local == "pPr" && in_para {
                    collect_list_level(event, &mut current_list_level);
                }
            }
            Ok(Event::Text(ref event)) if in_text => {
                let text = String::from_utf8_lossy(event.as_ref());
                current_text.push_str(&text);
                if in_cell {
                    current_cell_text.push_str(&text);
                }
            }
            Ok(Event::End(ref event)) => {
                let raw = String::from_utf8_lossy(event.name().as_ref()).to_string();
                let local = raw.rsplit(':').next().unwrap_or(&raw);
                match local {
                    "t" => in_text = false,
                    "r" => in_run = false,
                    "p" if in_tx_body => finish_paragraph(
                        &mut current_text,
                        current_list_level,
                        &mut current_paras,
                        &mut in_para,
                    ),
                    "txBody" => in_tx_body = false,
                    "sp" => finish_shape(
                        &mut title,
                        &mut elements,
                        &mut placeholder,
                        &mut current_paras,
                    ),
                    "tc" => {
                        in_cell = false;
                        current_row.push(std::mem::take(&mut current_cell_text));
                    }
                    "tr" => {
                        if !current_row.is_empty() {
                            table_rows.push(std::mem::take(&mut current_row));
                        }
                    }
                    "tbl" => {
                        if !table_rows.is_empty() {
                            elements.push(PptxElement::Table {
                                rows: std::mem::take(&mut table_rows),
                            });
                        }
                        in_table = false;
                    }
                    "pic" => {
                        elements.push(PptxElement::Image {
                            alt_text: picture_alt.take(),
                            embed_id: picture_embed_id.take(),
                            embedded_path: None,
                            media_b64: None,
                            mime_type: None,
                        });
                        in_picture = false;
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

    (title, elements)
}

fn collect_placeholder(
    event: &quick_xml::events::BytesStart<'_>,
    placeholder: &mut Option<String>,
) {
    for attr in event.attributes().flatten() {
        if attr.key.as_ref() == b"type" {
            *placeholder = Some(String::from_utf8_lossy(&attr.value).to_string());
        }
    }
}

fn collect_picture_alt(
    event: &quick_xml::events::BytesStart<'_>,
    picture_alt: &mut Option<String>,
) {
    for attr in event.attributes().flatten() {
        if attr.key.as_ref() == b"descr" {
            *picture_alt = Some(String::from_utf8_lossy(&attr.value).to_string());
        }
    }
}

fn collect_picture_embed_id(
    event: &quick_xml::events::BytesStart<'_>,
    picture_embed_id: &mut Option<String>,
) {
    for attr in event.attributes().flatten() {
        let key = String::from_utf8_lossy(attr.key.as_ref());
        if key == "r:embed" || key.ends_with(":embed") {
            *picture_embed_id = Some(String::from_utf8_lossy(&attr.value).to_string());
        }
    }
}

fn collect_list_level(event: &quick_xml::events::BytesStart<'_>, level: &mut Option<u32>) {
    for attr in event.attributes().flatten() {
        if attr.key.as_ref() == b"lvl"
            && let Ok(value) = String::from_utf8_lossy(&attr.value).parse::<u32>()
        {
            *level = Some(value);
        }
    }
}

fn finish_paragraph(
    text: &mut String,
    list_level: Option<u32>,
    paragraphs: &mut Vec<PptxTextPara>,
    in_paragraph: &mut bool,
) {
    if !text.trim().is_empty() {
        paragraphs.push(PptxTextPara {
            text: text.clone(),
            list_level,
        });
    }
    text.clear();
    *in_paragraph = false;
}

fn finish_shape(
    title: &mut Option<String>,
    elements: &mut Vec<PptxElement>,
    placeholder: &mut Option<String>,
    paragraphs: &mut Vec<PptxTextPara>,
) {
    if !paragraphs.is_empty() {
        if matches!(placeholder.as_deref(), Some("title" | "ctrTitle")) {
            if title.is_none() {
                *title = Some(
                    paragraphs
                        .iter()
                        .map(|paragraph| paragraph.text.clone())
                        .collect::<Vec<_>>()
                        .join(" "),
                );
            }
        } else {
            elements.push(PptxElement::TextBlock {
                placeholder: placeholder.clone(),
                paragraphs: paragraphs.clone(),
            });
        }
    }
    *placeholder = None;
    paragraphs.clear();
}
