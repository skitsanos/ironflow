use std::io::Read;

#[derive(Clone)]
pub(super) struct PptxSlide {
    pub(super) slide_index: u32,
    pub(super) title: Option<String>,
    pub(super) elements: Vec<PptxElement>,
    pub(super) speaker_notes: Option<String>,
    pub(super) comments: Vec<PptxComment>,
}

#[derive(Clone)]
pub(super) enum PptxElement {
    /// Top-level text block (could be the title placeholder or a content placeholder).
    /// `paragraphs` is a list of text paragraphs; each may be a bullet point with a level.
    TextBlock {
        placeholder: Option<String>,
        paragraphs: Vec<PptxTextPara>,
    },
    Table {
        rows: Vec<Vec<String>>,
    },
    Image {
        alt_text: Option<String>,
        embed_id: Option<String>,
        embedded_path: Option<String>,
        media_b64: Option<String>,
        mime_type: Option<String>,
    },
}

#[derive(Clone, Default)]
pub(super) struct PptxTextPara {
    pub(super) text: String,
    pub(super) list_level: Option<u32>,
}

#[derive(Clone, serde::Serialize, Default)]
pub(super) struct PptxComment {
    pub(super) slide_index: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) idx: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) author_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) initials: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) date: Option<String>,
    pub(super) text: String,
}

/// Parse a slideN.xml.rels file → map of rId → target path.
pub(super) fn parse_pptx_rels(xml: &str) -> std::collections::HashMap<String, String> {
    use quick_xml::events::Event;
    let mut reader = quick_xml::Reader::from_str(xml);
    let mut buf = Vec::new();
    let mut map = std::collections::HashMap::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e) | Event::Empty(ref e)) => {
                let raw = String::from_utf8_lossy(e.name().as_ref()).to_string();
                let local = raw.rsplit(':').next().unwrap_or(&raw);
                if local == "Relationship" {
                    let mut id = String::new();
                    let mut target = String::new();
                    for attr in e.attributes().flatten() {
                        let v = String::from_utf8_lossy(&attr.value).to_string();
                        match attr.key.as_ref() {
                            b"Id" => id = v,
                            b"Target" => target = v,
                            _ => {}
                        }
                    }
                    if !id.is_empty() && !target.is_empty() {
                        map.insert(id, target);
                    }
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

/// Resolve a relative rels target ("../media/image3.png") against its source dir.
pub(super) fn normalize_pptx_path(source_dir: &str, target: &str) -> String {
    // Split source_dir into components, drop empties.
    let mut parts: Vec<&str> = source_dir.split('/').filter(|s| !s.is_empty()).collect();
    for seg in target.split('/') {
        if seg == ".." {
            parts.pop();
        } else if seg != "." && !seg.is_empty() {
            parts.push(seg);
        }
    }
    parts.join("/")
}

pub(super) fn read_pptx_media(
    archive: &mut zip::ZipArchive<std::fs::File>,
    path: &str,
) -> Option<(Vec<u8>, String)> {
    let mut entry = archive.by_name(path).ok()?;
    let mut bytes = Vec::new();
    std::io::copy(&mut entry, &mut bytes).ok()?;
    let mime = match path
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_lowercase()
        .as_str()
    {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "tif" | "tiff" => "image/tiff",
        "svg" => "image/svg+xml",
        _ => "application/octet-stream",
    };
    Some((bytes, mime.to_string()))
}

/// Parse a single slide XML. Returns (title, elements).
pub(super) fn parse_pptx_slide(xml: &str) -> (Option<String>, Vec<PptxElement>) {
    use quick_xml::events::Event;
    let mut reader = quick_xml::Reader::from_str(xml);
    let mut buf = Vec::new();

    let mut title: Option<String> = None;
    let mut elements: Vec<PptxElement> = Vec::new();

    // Per-shape state
    let mut in_sp = false;
    let mut placeholder: Option<String> = None;
    let mut in_tx_body = false;
    let mut in_para = false;
    let mut current_text = String::new();
    let mut current_list_level: Option<u32> = None;
    let mut current_paras: Vec<PptxTextPara> = Vec::new();
    let mut in_run = false;
    let mut in_t = false;

    // Per-table state
    let mut in_tbl = false;
    let mut table_rows: Vec<Vec<String>> = Vec::new();
    let mut current_row: Vec<String> = Vec::new();
    let mut current_cell_text = String::new();
    let mut in_tc = false;

    // Per-picture state
    let mut in_pic = false;
    let mut pic_alt: Option<String> = None;
    let mut pic_embed_id: Option<String> = None;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e) | Event::Empty(ref e)) => {
                let raw = String::from_utf8_lossy(e.name().as_ref()).to_string();
                let local = raw.rsplit(':').next().unwrap_or(&raw);

                // Shape boundaries
                match local {
                    "sp" => {
                        in_sp = true;
                        placeholder = None;
                        current_paras.clear();
                    }
                    "ph" => {
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"type" {
                                placeholder =
                                    Some(String::from_utf8_lossy(&attr.value).to_string());
                            }
                        }
                    }
                    "txBody" => {
                        in_tx_body = true;
                    }
                    "p" if in_tx_body => {
                        in_para = true;
                        current_text.clear();
                        current_list_level = None;
                    }
                    "pPr" if in_para => {}
                    "r" if in_para => {
                        in_run = true;
                    }
                    "t" if in_run => {
                        in_t = true;
                    }
                    // Table parts
                    "tbl" => {
                        in_tbl = true;
                        table_rows.clear();
                    }
                    "tr" if in_tbl => {
                        current_row.clear();
                    }
                    "tc" if in_tbl => {
                        in_tc = true;
                        current_cell_text.clear();
                    }
                    "pic" => {
                        in_pic = true;
                        pic_alt = None;
                        pic_embed_id = None;
                    }
                    "cNvPr" if in_pic => {
                        // <p:cNvPr id="..." name="..." descr="alt-text"/>
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"descr" {
                                pic_alt = Some(String::from_utf8_lossy(&attr.value).to_string());
                            }
                        }
                    }
                    "blip" if in_pic => {
                        // <a:blip r:embed="rId3"/>
                        for attr in e.attributes().flatten() {
                            // r:embed has full key "r:embed"; check the local name
                            let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                            if key == "r:embed" || key.ends_with(":embed") {
                                pic_embed_id =
                                    Some(String::from_utf8_lossy(&attr.value).to_string());
                            }
                        }
                    }
                    _ => {}
                }

                // List level from pPr (a:pPr lvl="1")
                if local == "pPr" && in_para {
                    for attr in e.attributes().flatten() {
                        if attr.key.as_ref() == b"lvl"
                            && let Ok(lvl) = String::from_utf8_lossy(&attr.value).parse::<u32>()
                        {
                            current_list_level = Some(lvl);
                        }
                    }
                }
            }
            Ok(Event::Text(ref e)) if in_t => {
                let text = String::from_utf8_lossy(e.as_ref());
                current_text.push_str(&text);
                if in_tc {
                    current_cell_text.push_str(&text);
                }
            }
            Ok(Event::End(ref e)) => {
                let raw = String::from_utf8_lossy(e.name().as_ref()).to_string();
                let local = raw.rsplit(':').next().unwrap_or(&raw);
                match local {
                    "t" => {
                        in_t = false;
                    }
                    "r" => {
                        in_run = false;
                    }
                    "p" if in_tx_body => {
                        if !current_text.trim().is_empty() {
                            current_paras.push(PptxTextPara {
                                text: current_text.clone(),
                                list_level: current_list_level,
                            });
                        }
                        current_text.clear();
                        in_para = false;
                    }
                    "txBody" => {
                        in_tx_body = false;
                    }
                    "sp" => {
                        // Finalize this shape's text block
                        if !current_paras.is_empty() {
                            // Title placeholder → grab as slide title
                            if placeholder.as_deref() == Some("title")
                                || placeholder.as_deref() == Some("ctrTitle")
                            {
                                if title.is_none() {
                                    title = Some(
                                        current_paras
                                            .iter()
                                            .map(|p| p.text.clone())
                                            .collect::<Vec<_>>()
                                            .join(" "),
                                    );
                                }
                            } else {
                                elements.push(PptxElement::TextBlock {
                                    placeholder: placeholder.clone(),
                                    paragraphs: current_paras.clone(),
                                });
                            }
                        }
                        in_sp = false;
                        placeholder = None;
                        current_paras.clear();
                    }
                    "tc" => {
                        in_tc = false;
                        current_row.push(current_cell_text.clone());
                        current_cell_text.clear();
                    }
                    "tr" => {
                        if !current_row.is_empty() {
                            table_rows.push(current_row.clone());
                        }
                        current_row.clear();
                    }
                    "tbl" => {
                        if !table_rows.is_empty() {
                            elements.push(PptxElement::Table {
                                rows: table_rows.clone(),
                            });
                        }
                        in_tbl = false;
                        table_rows.clear();
                    }
                    "pic" => {
                        elements.push(PptxElement::Image {
                            alt_text: pic_alt.take(),
                            embed_id: pic_embed_id.take(),
                            embedded_path: None,
                            media_b64: None,
                            mime_type: None,
                        });
                        in_pic = false;
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

    let _ = in_sp; // silence unused warning if compiler complains
    (title, elements)
}

pub(super) fn parse_pptx_notes(xml: &str) -> String {
    use quick_xml::events::Event;
    let mut reader = quick_xml::Reader::from_str(xml);
    let mut buf = Vec::new();
    let mut text = String::new();
    let mut in_t = false;
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let raw = String::from_utf8_lossy(e.name().as_ref()).to_string();
                let local = raw.rsplit(':').next().unwrap_or(&raw);
                if local == "t" {
                    in_t = true;
                }
            }
            Ok(Event::Text(ref e)) if in_t => {
                text.push_str(&String::from_utf8_lossy(e.as_ref()));
                text.push('\n');
            }
            Ok(Event::End(_)) => {
                in_t = false;
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    text.trim().to_string()
}

pub(super) fn extract_pptx_comments(
    archive: &mut zip::ZipArchive<std::fs::File>,
) -> Vec<PptxComment> {
    // Author lookup (legacy: ppt/commentAuthors.xml).
    let mut authors: std::collections::HashMap<String, (Option<String>, Option<String>)> =
        std::collections::HashMap::new();
    if let Ok(mut entry) = archive.by_name("ppt/commentAuthors.xml") {
        let mut buf = String::new();
        entry.read_to_string(&mut buf).ok();
        if !buf.is_empty() {
            use quick_xml::events::Event;
            let mut reader = quick_xml::Reader::from_str(&buf);
            let mut rbuf = Vec::new();
            loop {
                match reader.read_event_into(&mut rbuf) {
                    Ok(Event::Start(ref e) | Event::Empty(ref e)) => {
                        let raw = String::from_utf8_lossy(e.name().as_ref()).to_string();
                        let local = raw.rsplit(':').next().unwrap_or(&raw);
                        if local == "cmAuthor" {
                            let mut id = String::new();
                            let mut name: Option<String> = None;
                            let mut initials: Option<String> = None;
                            for attr in e.attributes().flatten() {
                                let v = String::from_utf8_lossy(&attr.value).to_string();
                                match attr.key.as_ref() {
                                    b"id" => id = v,
                                    b"name" => name = Some(v),
                                    b"initials" => initials = Some(v),
                                    _ => {}
                                }
                            }
                            if !id.is_empty() {
                                authors.insert(id, (name, initials));
                            }
                        }
                    }
                    Ok(Event::Eof) => break,
                    Err(_) => break,
                    _ => {}
                }
                rbuf.clear();
            }
        }
    }

    // Find all ppt/comments/comment*.xml files and parse each.
    let mut comment_names: Vec<String> = (0..archive.len())
        .filter_map(|i| {
            let entry = archive.by_index(i).ok()?;
            let name = entry.name().to_string();
            if name.starts_with("ppt/comments/comment") && name.ends_with(".xml") {
                Some(name)
            } else {
                None
            }
        })
        .collect();
    comment_names.sort_by_key(|n| {
        n.trim_start_matches("ppt/comments/comment")
            .trim_end_matches(".xml")
            .parse::<u32>()
            .unwrap_or(0)
    });

    let mut out: Vec<PptxComment> = Vec::new();
    for name in &comment_names {
        // Derive slide_index from the comment file name (commentN.xml ↔ slideN.xml by convention).
        let slide_index = name
            .trim_start_matches("ppt/comments/comment")
            .trim_end_matches(".xml")
            .parse::<u32>()
            .unwrap_or(0);

        let xml = {
            let mut buf = String::new();
            if let Ok(mut entry) = archive.by_name(name) {
                entry.read_to_string(&mut buf).ok();
            }
            buf
        };
        if xml.is_empty() {
            continue;
        }

        // Parse <p:cm> entries; each has authorId, dt, idx attrs + a <p:text> child.
        use quick_xml::events::Event;
        let mut reader = quick_xml::Reader::from_str(&xml);
        let mut rbuf = Vec::new();
        let mut current: Option<PptxComment> = None;
        let mut in_text = false;
        loop {
            match reader.read_event_into(&mut rbuf) {
                Ok(Event::Start(ref e) | Event::Empty(ref e)) => {
                    let raw = String::from_utf8_lossy(e.name().as_ref()).to_string();
                    let local = raw.rsplit(':').next().unwrap_or(&raw);
                    if local == "cm" {
                        let mut c = PptxComment {
                            slide_index,
                            ..Default::default()
                        };
                        for attr in e.attributes().flatten() {
                            let v = String::from_utf8_lossy(&attr.value).to_string();
                            match attr.key.as_ref() {
                                b"authorId" => {
                                    if let Some((name, initials)) = authors.get(&v) {
                                        c.author = name.clone();
                                        c.initials = initials.clone();
                                    }
                                    c.author_id = Some(v);
                                }
                                b"dt" => c.date = Some(v),
                                b"idx" => c.idx = Some(v),
                                _ => {}
                            }
                        }
                        current = Some(c);
                    } else if local == "text" {
                        in_text = current.is_some();
                    }
                }
                Ok(Event::Text(ref e)) if in_text => {
                    if let Some(c) = current.as_mut() {
                        let t = String::from_utf8_lossy(e.as_ref());
                        c.text.push_str(&t);
                    }
                }
                Ok(Event::End(ref e)) => {
                    let raw = String::from_utf8_lossy(e.name().as_ref()).to_string();
                    let local = raw.rsplit(':').next().unwrap_or(&raw);
                    if local == "text" {
                        in_text = false;
                    } else if local == "cm"
                        && let Some(c) = current.take()
                    {
                        out.push(c);
                    }
                }
                Ok(Event::Eof) => break,
                Err(_) => break,
                _ => {}
            }
            rbuf.clear();
        }
    }

    out
}
