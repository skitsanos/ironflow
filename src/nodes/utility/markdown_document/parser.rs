use anyhow::{Result, bail, ensure};
use comrak::{
    Arena, Options,
    nodes::{ListType, Node, NodeValue},
};

use super::model::{Block, MAX_DEPTH, MAX_INPUT, MAX_NODES, Run, Style, safe_link, valid_text};
use crate::util::execution::ExecutionControl;

pub(super) fn parse(input: &str, execution: &ExecutionControl) -> Result<Vec<Block>> {
    ensure!(
        input.len() <= MAX_INPUT,
        "Markdown exceeds {MAX_INPUT} input bytes"
    );
    valid_text(input)?;
    let arena = Arena::new();
    let mut options = Options::default();
    options.extension.strikethrough = true;
    options.extension.table = true;
    options.extension.tasklist = true;
    options.extension.autolink = true;
    let root = comrak::try_parse_document(&arena, input, &options, &|| {
        execution.is_cancelled() || execution.deadline_exceeded()
    })?;
    let mut count = 0;
    for edge in root.traverse() {
        execution.checkpoint()?;
        if let comrak::arena_tree::NodeEdge::Start(node) = edge {
            count += 1;
            ensure!(count <= MAX_NODES, "Markdown exceeds {MAX_NODES} AST nodes");
            ensure!(
                node.ancestors().take(MAX_DEPTH + 2).count() <= MAX_DEPTH + 1,
                "Markdown nesting exceeds {MAX_DEPTH}"
            );
        }
    }
    let mut blocks = Vec::new();
    walk(root, 0, "", &mut blocks, execution)?;
    Ok(blocks)
}

#[allow(clippy::too_many_lines)] // One recursive AST visitor; each node kind is one arm.
fn walk<'a>(
    node: Node<'a>,
    depth: usize,
    label: &str,
    blocks: &mut Vec<Block>,
    execution: &ExecutionControl,
) -> Result<()> {
    execution.checkpoint()?;
    match &node.data.borrow().value {
        NodeValue::Document | NodeValue::Item(_) | NodeValue::TaskItem(_) => {
            let mut first = true;
            for child in node.children() {
                if first
                    && !label.is_empty()
                    && !matches!(child.data.borrow().value, NodeValue::Paragraph)
                {
                    blocks.push(Block::Paragraph {
                        runs: Vec::new(),
                        depth,
                        label: label.to_owned(),
                    });
                    first = false;
                }
                walk(
                    child,
                    depth,
                    if first { label } else { "" },
                    blocks,
                    execution,
                )?;
                first = false;
            }
            if first && !label.is_empty() {
                blocks.push(Block::Paragraph {
                    runs: Vec::new(),
                    depth,
                    label: label.to_owned(),
                });
            }
        }
        NodeValue::BlockQuote => {
            for child in node.children() {
                walk(child, depth + 1, "> ", blocks, execution)?;
            }
        }
        NodeValue::List(list) => {
            for (index, item) in node.children().enumerate() {
                let marker = match &item.data.borrow().value {
                    NodeValue::TaskItem(task) => {
                        if task.symbol.is_some() {
                            "[x] ".to_owned()
                        } else {
                            "[ ] ".to_owned()
                        }
                    }
                    _ if list.list_type == ListType::Ordered => {
                        format!("{}. ", list.start.saturating_add(index))
                    }
                    _ => "- ".to_owned(),
                };
                walk(item, depth + 1, &marker, blocks, execution)?;
            }
        }
        NodeValue::Paragraph => {
            let mut runs = Vec::new();
            let mut marker = label.to_owned();
            for child in node.children() {
                if let NodeValue::Image(image) = &child.data.borrow().value {
                    ensure!(
                        depth == 0,
                        "images inside lists or quotes are not supported; use a standalone image paragraph"
                    );
                    flush(&mut runs, depth, &mut marker, blocks);
                    let alt = inline(child, Style::default())?
                        .into_iter()
                        .map(|r| r.text)
                        .collect();
                    blocks.push(Block::Image {
                        key: image.url.clone(),
                        alt,
                    });
                } else {
                    runs.extend(inline(child, Style::default())?);
                }
            }
            flush(&mut runs, depth, &mut marker, blocks);
        }
        NodeValue::Heading(heading) => blocks.push(Block::Heading(
            heading.level,
            inline(node, Style::default())?,
        )),
        NodeValue::CodeBlock(code) => blocks.push(Block::Code(code.literal.clone())),
        NodeValue::ThematicBreak => blocks.push(Block::Rule),
        NodeValue::Table(_) => {
            let mut rows = Vec::new();
            for row in node.children() {
                ensure!(rows.len() < 2_000, "table exceeds 2000 rows");
                let mut cells = Vec::new();
                for cell in row.children() {
                    ensure!(cells.len() < 32, "table exceeds 32 columns");
                    cells.push(inline(cell, Style::default())?);
                }
                rows.push(cells);
            }
            blocks.push(Block::Table(rows));
        }
        NodeValue::HtmlBlock(_) | NodeValue::HtmlInline(_) => {
            bail!("raw HTML is not supported in Markdown documents")
        }
        other => bail!("unsupported Markdown block: {}", other.xml_node_name()),
    }
    Ok(())
}

fn flush(runs: &mut Vec<Run>, depth: usize, label: &mut String, blocks: &mut Vec<Block>) {
    if !runs.is_empty() {
        blocks.push(Block::Paragraph {
            runs: std::mem::take(runs),
            depth,
            label: std::mem::take(label),
        });
    }
}

fn inline<'a>(node: Node<'a>, mut style: Style) -> Result<Vec<Run>> {
    let value = &node.data.borrow().value;
    let text = match value {
        NodeValue::Text(text) => Some(text.to_string()),
        NodeValue::Code(code) => {
            style.code = true;
            Some(code.literal.clone())
        }
        NodeValue::SoftBreak => Some(" ".to_owned()),
        NodeValue::LineBreak => Some("\n".to_owned()),
        NodeValue::Strong => {
            style.bold = true;
            None
        }
        NodeValue::Emph => {
            style.italic = true;
            None
        }
        NodeValue::Strikethrough => {
            style.strike = true;
            None
        }
        NodeValue::Link(link) => {
            safe_link(&link.url)?;
            style.link = Some(link.url.as_str().into());
            None
        }
        NodeValue::Image(_) => {
            ensure!(
                node.parent()
                    .is_some_and(|p| matches!(p.data.borrow().value, NodeValue::Paragraph)),
                "nested inline images are not supported"
            );
            None
        }
        NodeValue::Paragraph | NodeValue::Heading(_) | NodeValue::TableCell => None,
        NodeValue::HtmlInline(_) => bail!("raw HTML is not supported in Markdown documents"),
        other => bail!("unsupported Markdown inline: {}", other.xml_node_name()),
    };
    if let Some(text) = text {
        return Ok(vec![Run { text, style }]);
    }
    let mut runs = Vec::new();
    for child in node.children() {
        runs.extend(inline(child, style.clone())?);
    }
    Ok(runs)
}
