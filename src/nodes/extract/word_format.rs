use super::docx_parser::{DocxBlock, DocxCell, DocxParagraph, DocxRun, DocxTable};

pub(super) fn paragraph_plain_text(para: &DocxParagraph) -> String {
    para.runs.iter().map(|r| r.text.as_str()).collect()
}

pub(super) fn blocks_to_text(blocks: &[DocxBlock]) -> String {
    let mut lines: Vec<String> = Vec::new();
    for block in blocks {
        match block {
            DocxBlock::Paragraph(p) => {
                let text = paragraph_plain_text(p);
                if !text.is_empty() || !p.runs.is_empty() {
                    lines.push(text);
                }
            }
            DocxBlock::Table(t) => {
                for row in &t.rows {
                    let row_text: Vec<String> = row
                        .cells
                        .iter()
                        .map(|c| {
                            c.paragraphs
                                .iter()
                                .map(paragraph_plain_text)
                                .filter(|s| !s.is_empty())
                                .collect::<Vec<_>>()
                                .join(" ")
                        })
                        .collect();
                    lines.push(row_text.join(" | "));
                }
            }
        }
    }
    lines.join("\n")
}

pub(super) fn blocks_to_markdown(blocks: &[DocxBlock]) -> String {
    let mut lines: Vec<String> = Vec::new();
    let mut numbered_counters: std::collections::HashMap<u32, u32> =
        std::collections::HashMap::new();

    for block in blocks {
        match block {
            DocxBlock::Paragraph(para) => {
                let text: String = para.runs.iter().map(format_run_markdown).collect();

                if text.is_empty() && para.runs.is_empty() {
                    continue;
                }

                // Heading styles
                if let Some(ref style) = para.style {
                    let heading_level = match style.as_str() {
                        "Heading1" | "heading1" | "Title" => Some(1),
                        "Heading2" | "heading2" | "Subtitle" => Some(2),
                        "Heading3" | "heading3" => Some(3),
                        "Heading4" | "heading4" => Some(4),
                        "Heading5" | "heading5" => Some(5),
                        "Heading6" | "heading6" => Some(6),
                        _ => None,
                    };

                    if let Some(level) = heading_level {
                        lines.push(String::new());
                        lines.push(format!("{} {}", "#".repeat(level), text));
                        lines.push(String::new());
                        continue;
                    }
                }

                // List items
                if para.is_list_item {
                    let indent = "  ".repeat(para.list_level as usize);
                    if para.is_numbered {
                        let counter = numbered_counters.entry(para.list_level).or_insert(0);
                        *counter += 1;
                        lines.push(format!("{}{}. {}", indent, counter, text));
                    } else {
                        lines.push(format!("{}- {}", indent, text));
                    }
                    continue;
                }

                numbered_counters.clear();
                lines.push(text);
            }
            DocxBlock::Table(t) => {
                numbered_counters.clear();
                let rendered = table_to_markdown(t);
                if !rendered.is_empty() {
                    lines.push(String::new());
                    lines.push(rendered);
                    lines.push(String::new());
                }
            }
        }
    }

    lines.join("\n").trim().to_string()
}

fn table_to_markdown(table: &DocxTable) -> String {
    if table.rows.is_empty() {
        return String::new();
    }
    let mut out: Vec<String> = Vec::new();
    let column_count = table.rows.iter().map(|r| r.cells.len()).max().unwrap_or(0);
    if column_count == 0 {
        return String::new();
    }

    let render_cell = |cell: &DocxCell| -> String {
        cell.paragraphs
            .iter()
            .map(|p| p.runs.iter().map(format_run_markdown).collect::<String>())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("<br>")
            .replace('|', "\\|")
    };

    let format_row = |cells: &[DocxCell]| -> String {
        let mut parts: Vec<String> = cells.iter().map(render_cell).collect();
        while parts.len() < column_count {
            parts.push(String::new());
        }
        format!("| {} |", parts.join(" | "))
    };

    out.push(format_row(&table.rows[0].cells));
    out.push(format!("|{}|", vec![" --- "; column_count].join("|")));
    for row in &table.rows[1..] {
        out.push(format_row(&row.cells));
    }
    out.join("\n")
}

fn format_run_markdown(run: &DocxRun) -> String {
    let mut text = run.text.clone();
    if text.is_empty() {
        return text;
    }

    if run.strikethrough {
        text = format!("~~{}~~", text);
    }
    if run.bold && run.italic {
        text = format!("***{}***", text);
    } else if run.bold {
        text = format!("**{}**", text);
    } else if run.italic {
        text = format!("*{}*", text);
    }
    // Underline has no standard markdown — skip

    text
}

pub(super) fn blocks_to_json(blocks: &[DocxBlock]) -> serde_json::Value {
    let arr: Vec<serde_json::Value> = blocks
        .iter()
        .enumerate()
        .map(|(i, b)| match b {
            DocxBlock::Paragraph(p) => paragraph_to_json(p, i),
            DocxBlock::Table(t) => table_to_json(t, i),
        })
        .collect();
    serde_json::json!({ "blocks": arr })
}

fn run_to_json(run: &DocxRun) -> serde_json::Value {
    let mut obj = serde_json::Map::new();
    obj.insert("text".into(), serde_json::Value::String(run.text.clone()));
    if run.bold {
        obj.insert("bold".into(), serde_json::Value::Bool(true));
    }
    if run.italic {
        obj.insert("italic".into(), serde_json::Value::Bool(true));
    }
    if run.underline {
        obj.insert("underline".into(), serde_json::Value::Bool(true));
    }
    if run.strikethrough {
        obj.insert("strike".into(), serde_json::Value::Bool(true));
    }
    if let Some(ref c) = run.color {
        obj.insert("color".into(), serde_json::Value::String(c.clone()));
    }
    if let Some(ref h) = run.highlight {
        obj.insert("highlight".into(), serde_json::Value::String(h.clone()));
    }
    serde_json::Value::Object(obj)
}

fn paragraph_to_json(para: &DocxParagraph, index: usize) -> serde_json::Value {
    let runs: Vec<serde_json::Value> = para.runs.iter().map(run_to_json).collect();
    let text = paragraph_plain_text(para);

    let mut colors: Vec<String> = para.runs.iter().filter_map(|r| r.color.clone()).collect();
    colors.sort();
    colors.dedup();

    let mut obj = serde_json::Map::new();
    obj.insert("type".into(), serde_json::Value::String("paragraph".into()));
    obj.insert("index".into(), serde_json::json!(index));
    if let Some(ref s) = para.style {
        obj.insert("style".into(), serde_json::Value::String(s.clone()));
    }
    if para.is_list_item {
        obj.insert(
            "list".into(),
            serde_json::json!({
                "level": para.list_level,
                "numbered": para.is_numbered,
            }),
        );
    }
    if !colors.is_empty() {
        obj.insert("colors".into(), serde_json::json!(colors));
    }
    obj.insert("runs".into(), serde_json::Value::Array(runs));
    obj.insert("text".into(), serde_json::Value::String(text));
    serde_json::Value::Object(obj)
}

fn cell_to_json(cell: &DocxCell) -> serde_json::Value {
    let paragraphs: Vec<serde_json::Value> = cell
        .paragraphs
        .iter()
        .enumerate()
        .map(|(i, p)| paragraph_to_json(p, i))
        .collect();
    serde_json::json!({ "paragraphs": paragraphs })
}

fn table_to_json(table: &DocxTable, index: usize) -> serde_json::Value {
    let rows: Vec<serde_json::Value> = table
        .rows
        .iter()
        .map(|row| {
            let cells: Vec<serde_json::Value> = row.cells.iter().map(cell_to_json).collect();
            serde_json::json!({ "cells": cells })
        })
        .collect();
    serde_json::json!({
        "type": "table",
        "index": index,
        "rows": rows,
    })
}
