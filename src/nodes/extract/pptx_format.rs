use super::pptx_parser::{PptxElement, PptxSlide};

pub(super) fn pptx_slides_to_text(slides: &[PptxSlide]) -> String {
    let mut parts: Vec<String> = Vec::new();
    for s in slides {
        parts.push(format!("===== SLIDE {} =====", s.slide_index));
        if let Some(ref t) = s.title {
            parts.push(t.clone());
        }
        for el in &s.elements {
            match el {
                PptxElement::TextBlock { paragraphs, .. } => {
                    for p in paragraphs {
                        parts.push(p.text.clone());
                    }
                }
                PptxElement::Table { rows } => {
                    for row in rows {
                        parts.push(row.join(" | "));
                    }
                }
                PptxElement::Image { .. } => {}
            }
        }
        if let Some(ref n) = s.speaker_notes {
            parts.push("--- NOTES ---".to_string());
            parts.push(n.clone());
        }
        for c in &s.comments {
            parts.push(format!(
                "[COMMENT by {}{}]: {}",
                c.author.clone().unwrap_or_else(|| "?".into()),
                c.date
                    .clone()
                    .map(|d| format!(" @ {}", d))
                    .unwrap_or_default(),
                c.text
            ));
        }
    }
    parts.join("\n")
}

pub(super) fn pptx_slides_to_markdown(slides: &[PptxSlide]) -> String {
    let mut out: Vec<String> = Vec::new();
    for s in slides {
        out.push(format!("## Slide {}", s.slide_index));
        out.push(String::new());
        if let Some(ref t) = s.title {
            out.push(format!("### {}", t));
            out.push(String::new());
        }
        for el in &s.elements {
            match el {
                PptxElement::TextBlock { paragraphs, .. } => {
                    for p in paragraphs {
                        if let Some(lvl) = p.list_level {
                            let indent = "  ".repeat(lvl as usize);
                            out.push(format!("{}- {}", indent, p.text));
                        } else {
                            out.push(p.text.clone());
                        }
                    }
                    out.push(String::new());
                }
                PptxElement::Table { rows } => {
                    if !rows.is_empty() {
                        out.push(format!("| {} |", rows[0].join(" | ")));
                        out.push(format!("|{}|", vec![" --- "; rows[0].len()].join("|")));
                        for row in &rows[1..] {
                            out.push(format!("| {} |", row.join(" | ")));
                        }
                        out.push(String::new());
                    }
                }
                PptxElement::Image { .. } => {
                    out.push("*(image)*".to_string());
                }
            }
        }
        if let Some(ref n) = s.speaker_notes {
            out.push("**Speaker notes:**".to_string());
            out.push(n.clone());
            out.push(String::new());
        }
        for c in &s.comments {
            out.push(format!(
                "> 💬 **{}**{}: {}",
                c.author.clone().unwrap_or_else(|| "?".into()),
                c.date
                    .clone()
                    .map(|d| format!(" ({})", d))
                    .unwrap_or_default(),
                c.text
            ));
        }
    }
    out.join("\n").trim().to_string()
}

pub(super) fn pptx_slides_to_json(slides: &[PptxSlide]) -> serde_json::Value {
    let arr: Vec<serde_json::Value> = slides
        .iter()
        .map(|s| {
            let elements: Vec<serde_json::Value> = s
                .elements
                .iter()
                .map(|el| match el {
                    PptxElement::TextBlock {
                        placeholder,
                        paragraphs,
                    } => {
                        let paras: Vec<serde_json::Value> = paragraphs
                            .iter()
                            .map(|p| {
                                let mut o = serde_json::Map::new();
                                o.insert("text".into(), serde_json::Value::String(p.text.clone()));
                                if let Some(lvl) = p.list_level {
                                    o.insert("list_level".into(), serde_json::json!(lvl));
                                }
                                serde_json::Value::Object(o)
                            })
                            .collect();
                        let mut o = serde_json::Map::new();
                        o.insert(
                            "type".into(),
                            serde_json::Value::String("text_block".into()),
                        );
                        if let Some(p) = placeholder {
                            o.insert("placeholder".into(), serde_json::Value::String(p.clone()));
                        }
                        o.insert("paragraphs".into(), serde_json::Value::Array(paras));
                        serde_json::Value::Object(o)
                    }
                    PptxElement::Table { rows } => {
                        serde_json::json!({
                            "type": "table",
                            "rows": rows
                        })
                    }
                    PptxElement::Image {
                        alt_text,
                        embed_id,
                        embedded_path,
                        media_b64,
                        mime_type,
                    } => {
                        let mut o = serde_json::Map::new();
                        o.insert("type".into(), serde_json::Value::String("image".into()));
                        if let Some(a) = alt_text {
                            o.insert("alt_text".into(), serde_json::Value::String(a.clone()));
                        }
                        if let Some(e) = embed_id {
                            o.insert("embed_id".into(), serde_json::Value::String(e.clone()));
                        }
                        if let Some(p) = embedded_path {
                            o.insert("embedded_path".into(), serde_json::Value::String(p.clone()));
                        }
                        if let Some(m) = media_b64 {
                            o.insert("media_b64".into(), serde_json::Value::String(m.clone()));
                        }
                        if let Some(mt) = mime_type {
                            o.insert("mime_type".into(), serde_json::Value::String(mt.clone()));
                        }
                        serde_json::Value::Object(o)
                    }
                })
                .collect();
            let comments: Vec<serde_json::Value> = s
                .comments
                .iter()
                .map(|c| serde_json::to_value(c).unwrap_or(serde_json::Value::Null))
                .collect();
            let mut obj = serde_json::Map::new();
            obj.insert("slide_index".into(), serde_json::json!(s.slide_index));
            if let Some(ref t) = s.title {
                obj.insert("title".into(), serde_json::Value::String(t.clone()));
            }
            obj.insert("elements".into(), serde_json::Value::Array(elements));
            if let Some(ref n) = s.speaker_notes {
                obj.insert("speaker_notes".into(), serde_json::Value::String(n.clone()));
            }
            if !comments.is_empty() {
                obj.insert("comments".into(), serde_json::Value::Array(comments));
            }
            serde_json::Value::Object(obj)
        })
        .collect();
    serde_json::json!({ "slides": arr })
}
