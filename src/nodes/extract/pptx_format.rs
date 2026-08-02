mod text;

use anyhow::Result;

use super::pptx_parser::{PptxComment, PptxElement, PptxSlide};
use super::resource::Budget;

pub(super) use text::{pptx_slides_to_markdown, pptx_slides_to_text};

pub(super) fn pptx_slides_into_json(
    slides: Vec<PptxSlide>,
    budget: &mut Budget<'_>,
) -> Result<serde_json::Value> {
    let mut serialized_slides = Vec::new();
    serialized_slides.try_reserve_exact(slides.len())?;
    for slide in slides {
        let PptxSlide {
            slide_index,
            title,
            elements: slide_elements,
            speaker_notes,
            comments,
        } = slide;
        budget.checkpoint()?;
        budget.charge_output(48, "PPTX JSON structure")?;
        let mut elements = Vec::new();
        elements.try_reserve_exact(slide_elements.len())?;
        for element in slide_elements {
            budget.checkpoint()?;
            elements.push(element_into_json(element, budget)?);
        }
        let mut object = serde_json::Map::new();
        object.insert("slide_index".into(), serde_json::json!(slide_index));
        if let Some(title) = title {
            object.insert("title".into(), serde_json::Value::String(title));
        }
        object.insert("elements".into(), serde_json::Value::Array(elements));
        if let Some(notes) = speaker_notes {
            object.insert("speaker_notes".into(), serde_json::Value::String(notes));
        }
        if !comments.is_empty() {
            object.insert(
                "comments".into(),
                pptx_comments_into_json(comments, budget)?,
            );
        }
        serialized_slides.push(serde_json::Value::Object(object));
    }
    Ok(serde_json::json!({ "slides": serialized_slides }))
}

fn element_into_json(element: PptxElement, budget: &mut Budget<'_>) -> Result<serde_json::Value> {
    budget.charge_output(48, "PPTX JSON element structure")?;
    Ok(match element {
        PptxElement::TextBlock {
            placeholder,
            paragraphs,
        } => {
            let mut values = Vec::new();
            values.try_reserve_exact(paragraphs.len())?;
            for paragraph in paragraphs {
                budget.checkpoint()?;
                budget.charge_output(32, "PPTX JSON paragraph structure")?;
                let mut value = serde_json::Map::new();
                value.insert("text".into(), serde_json::Value::String(paragraph.text));
                if let Some(level) = paragraph.list_level {
                    value.insert("list_level".into(), serde_json::json!(level));
                }
                values.push(serde_json::Value::Object(value));
            }
            let mut value = serde_json::Map::new();
            value.insert(
                "type".into(),
                serde_json::Value::String("text_block".into()),
            );
            if let Some(placeholder) = placeholder {
                value.insert("placeholder".into(), serde_json::Value::String(placeholder));
            }
            value.insert("paragraphs".into(), serde_json::Value::Array(values));
            serde_json::Value::Object(value)
        }
        PptxElement::Table { rows } => table_into_json(rows, budget)?,
        PptxElement::Image {
            alt_text,
            embed_id,
            embedded_path,
            artifact,
        } => image_into_json(alt_text, embed_id, embedded_path, artifact)?,
    })
}

fn table_into_json(rows: Vec<Vec<String>>, budget: &mut Budget<'_>) -> Result<serde_json::Value> {
    let mut serialized_rows = Vec::new();
    serialized_rows.try_reserve_exact(rows.len())?;
    for row in rows {
        budget.checkpoint()?;
        budget.charge_output(2, "PPTX JSON table structure")?;
        let mut serialized_cells = Vec::new();
        serialized_cells.try_reserve_exact(row.len())?;
        for cell in row {
            budget.checkpoint()?;
            budget.charge_output(3, "PPTX JSON table structure")?;
            serialized_cells.push(serde_json::Value::String(cell));
        }
        serialized_rows.push(serde_json::Value::Array(serialized_cells));
    }
    Ok(serde_json::json!({ "type": "table", "rows": serialized_rows }))
}

fn pptx_comments_into_json(
    comments: Vec<PptxComment>,
    budget: &mut Budget<'_>,
) -> Result<serde_json::Value> {
    let mut values = Vec::new();
    values.try_reserve_exact(comments.len())?;
    for comment in comments {
        budget.checkpoint()?;
        budget.charge_output(64, "PPTX JSON comment structure")?;
        values.push(serde_json::to_value(comment)?);
    }
    Ok(serde_json::Value::Array(values))
}

pub(super) fn pptx_comments_to_json(
    comments: &[PptxComment],
    budget: &mut Budget<'_>,
    duplicate_retained_fields: bool,
) -> Result<serde_json::Value> {
    let mut values = Vec::new();
    values.try_reserve_exact(comments.len())?;
    for comment in comments {
        budget.checkpoint()?;
        budget.charge_output(64, "PPTX JSON comment structure")?;
        if duplicate_retained_fields {
            budget.charge_output(
                comment_retained_bytes(comment),
                "PPTX duplicated flat comments",
            )?;
        }
        values.push(serde_json::to_value(comment)?);
    }
    Ok(serde_json::Value::Array(values))
}

fn comment_retained_bytes(comment: &PptxComment) -> u64 {
    [
        comment.idx.as_deref(),
        comment.author_id.as_deref(),
        comment.author.as_deref(),
        comment.initials.as_deref(),
        comment.date.as_deref(),
        Some(comment.text.as_str()),
    ]
    .into_iter()
    .flatten()
    .map(|value| value.len() as u64)
    .fold(0, u64::saturating_add)
}

fn image_into_json(
    alt_text: Option<String>,
    embed_id: Option<String>,
    embedded_path: Option<String>,
    artifact: Option<crate::artifacts::ArtifactRef>,
) -> Result<serde_json::Value> {
    let mut value = serde_json::Map::new();
    value.insert("type".into(), serde_json::Value::String("image".into()));
    for (key, content) in [
        ("alt_text", alt_text),
        ("embed_id", embed_id),
        ("embedded_path", embedded_path),
    ] {
        if let Some(content) = content {
            value.insert(key.into(), serde_json::Value::String(content));
        }
    }
    if let Some(artifact) = artifact {
        value.insert("artifact".into(), serde_json::to_value(artifact)?);
    }
    Ok(serde_json::Value::Object(value))
}
