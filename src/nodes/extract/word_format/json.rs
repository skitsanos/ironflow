use anyhow::{Context, Result};

use super::super::docx_parser::{DocxBlock, DocxCell, DocxParagraph, DocxRun, DocxTable};
use super::super::resource::Budget;

pub(in crate::nodes::extract) fn blocks_to_json(
    blocks: &[DocxBlock],
    budget: &mut Budget<'_>,
) -> Result<serde_json::Value> {
    let mut values = Vec::new();
    values
        .try_reserve_exact(blocks.len())
        .context("extract_word: cannot reserve memory for JSON blocks")?;
    for (index, block) in blocks.iter().enumerate() {
        budget.checkpoint()?;
        values.push(match block {
            DocxBlock::Paragraph(paragraph) => paragraph_to_json(paragraph, index, budget)?,
            DocxBlock::Table(table) => table_to_json(table, index, budget)?,
        });
    }

    let mut object = serde_json::Map::new();
    insert_value(
        &mut object,
        "blocks",
        serde_json::Value::Array(values),
        budget,
    )?;
    Ok(serde_json::Value::Object(object))
}

fn run_to_json(run: &DocxRun, budget: &mut Budget<'_>) -> Result<serde_json::Value> {
    budget.checkpoint()?;
    let mut object = serde_json::Map::new();
    insert_string(
        &mut object,
        "text",
        &run.text,
        "DOCX JSON run text copies",
        budget,
    )?;
    if run.bold {
        insert_bool(&mut object, "bold", budget)?;
    }
    if run.italic {
        insert_bool(&mut object, "italic", budget)?;
    }
    if run.underline {
        insert_bool(&mut object, "underline", budget)?;
    }
    if run.strikethrough {
        insert_bool(&mut object, "strike", budget)?;
    }
    if let Some(color) = &run.color {
        insert_string(
            &mut object,
            "color",
            color,
            "DOCX JSON run color copies",
            budget,
        )?;
    }
    if let Some(highlight) = &run.highlight {
        insert_string(
            &mut object,
            "highlight",
            highlight,
            "DOCX JSON run highlight copies",
            budget,
        )?;
    }
    Ok(serde_json::Value::Object(object))
}

fn paragraph_to_json(
    paragraph: &DocxParagraph,
    index: usize,
    budget: &mut Budget<'_>,
) -> Result<serde_json::Value> {
    let mut runs = Vec::new();
    runs.try_reserve_exact(paragraph.runs.len())
        .context("extract_word: cannot reserve memory for JSON runs")?;
    for run in &paragraph.runs {
        budget.checkpoint()?;
        runs.push(run_to_json(run, budget)?);
    }
    let text = paragraph_text(paragraph, budget)?;
    let colors = paragraph_colors(paragraph, budget)?;

    let mut object = serde_json::Map::new();
    insert_string(
        &mut object,
        "type",
        "paragraph",
        "DOCX JSON paragraph type",
        budget,
    )?;
    insert_number(&mut object, "index", index, budget)?;
    if let Some(style) = &paragraph.style {
        insert_string(
            &mut object,
            "style",
            style,
            "DOCX JSON paragraph style copies",
            budget,
        )?;
    }
    if paragraph.is_list_item {
        let mut list = serde_json::Map::new();
        insert_number(&mut list, "level", paragraph.list_level as usize, budget)?;
        insert_value(
            &mut list,
            "numbered",
            serde_json::Value::Bool(paragraph.is_numbered),
            budget,
        )?;
        insert_value(&mut object, "list", serde_json::Value::Object(list), budget)?;
    }
    if !colors.is_empty() {
        insert_value(
            &mut object,
            "colors",
            serde_json::Value::Array(colors),
            budget,
        )?;
    }
    insert_value(&mut object, "runs", serde_json::Value::Array(runs), budget)?;
    insert_value(&mut object, "text", serde_json::Value::String(text), budget)?;
    Ok(serde_json::Value::Object(object))
}

fn paragraph_text(paragraph: &DocxParagraph, budget: &mut Budget<'_>) -> Result<String> {
    let mut length = 0_usize;
    for run in &paragraph.runs {
        budget.checkpoint()?;
        length = length.checked_add(run.text.len()).ok_or_else(|| {
            anyhow::anyhow!("extract_word: paragraph text length exceeds platform capacity")
        })?;
    }
    budget.charge_output(length as u64, "DOCX JSON paragraph text copies")?;
    let mut text = String::new();
    text.try_reserve_exact(length)
        .context("extract_word: cannot reserve memory for JSON paragraph text")?;
    for run in &paragraph.runs {
        budget.checkpoint()?;
        text.push_str(&run.text);
    }
    Ok(text)
}

fn paragraph_colors(
    paragraph: &DocxParagraph,
    budget: &mut Budget<'_>,
) -> Result<Vec<serde_json::Value>> {
    let mut colors = Vec::<&str>::new();
    colors
        .try_reserve_exact(paragraph.runs.len())
        .context("extract_word: cannot reserve memory for paragraph colors")?;
    for run in &paragraph.runs {
        budget.checkpoint()?;
        if let Some(color) = &run.color {
            colors.push(color);
        }
    }
    budget.checkpoint()?;
    colors.sort_unstable();
    colors.dedup();

    let mut values = Vec::new();
    values
        .try_reserve_exact(colors.len())
        .context("extract_word: cannot reserve memory for JSON paragraph colors")?;
    for color in colors {
        budget.checkpoint()?;
        budget.charge_output(color.len() as u64, "DOCX JSON paragraph color copies")?;
        values.push(serde_json::Value::String(color.to_owned()));
    }
    Ok(values)
}

fn cell_to_json(cell: &DocxCell, budget: &mut Budget<'_>) -> Result<serde_json::Value> {
    let mut paragraphs = Vec::new();
    paragraphs
        .try_reserve_exact(cell.paragraphs.len())
        .context("extract_word: cannot reserve memory for JSON cell paragraphs")?;
    for (index, paragraph) in cell.paragraphs.iter().enumerate() {
        budget.checkpoint()?;
        paragraphs.push(paragraph_to_json(paragraph, index, budget)?);
    }
    let mut object = serde_json::Map::new();
    insert_value(
        &mut object,
        "paragraphs",
        serde_json::Value::Array(paragraphs),
        budget,
    )?;
    Ok(serde_json::Value::Object(object))
}

fn table_to_json(
    table: &DocxTable,
    index: usize,
    budget: &mut Budget<'_>,
) -> Result<serde_json::Value> {
    let mut rows = Vec::new();
    rows.try_reserve_exact(table.rows.len())
        .context("extract_word: cannot reserve memory for JSON table rows")?;
    for row in &table.rows {
        budget.checkpoint()?;
        let mut cells = Vec::new();
        cells
            .try_reserve_exact(row.cells.len())
            .context("extract_word: cannot reserve memory for JSON table cells")?;
        for cell in &row.cells {
            budget.checkpoint()?;
            cells.push(cell_to_json(cell, budget)?);
        }
        let mut object = serde_json::Map::new();
        insert_value(
            &mut object,
            "cells",
            serde_json::Value::Array(cells),
            budget,
        )?;
        rows.push(serde_json::Value::Object(object));
    }

    let mut object = serde_json::Map::new();
    insert_string(&mut object, "type", "table", "DOCX JSON table type", budget)?;
    insert_number(&mut object, "index", index, budget)?;
    insert_value(&mut object, "rows", serde_json::Value::Array(rows), budget)?;
    Ok(serde_json::Value::Object(object))
}

fn insert_string(
    object: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    value: &str,
    what: &str,
    budget: &mut Budget<'_>,
) -> Result<()> {
    let bytes = key.len().saturating_add(value.len()) as u64;
    budget.charge_output(bytes, what)?;
    object.insert(key.to_owned(), serde_json::Value::String(value.to_owned()));
    Ok(())
}

fn insert_bool(
    object: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    budget: &mut Budget<'_>,
) -> Result<()> {
    insert_value(object, key, serde_json::Value::Bool(true), budget)
}

fn insert_number(
    object: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    value: usize,
    budget: &mut Budget<'_>,
) -> Result<()> {
    insert_value(object, key, serde_json::json!(value), budget)
}

fn insert_value(
    object: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    value: serde_json::Value,
    budget: &mut Budget<'_>,
) -> Result<()> {
    budget.charge_output(key.len() as u64, "DOCX JSON object keys")?;
    object.insert(key.to_owned(), value);
    Ok(())
}
