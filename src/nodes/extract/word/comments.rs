use std::collections::{HashMap, HashSet};
use std::io::BufRead;

use anyhow::Result;

use super::super::docx_parser::{XmlDocument, visit_attributes};
use super::super::resource::Budget;

#[derive(serde::Serialize, Default)]
pub(super) struct DocxComment {
    id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    initials: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    date: Option<String>,
    text: String,
    /// Source-document text between the comment range markers.
    #[serde(skip_serializing_if = "Option::is_none")]
    anchored_text: Option<String>,
}

pub(super) fn parse_docx_comments<R: BufRead>(
    comments_xml: R,
    budget: &mut Budget<'_>,
) -> Result<Vec<DocxComment>> {
    parse_comments(comments_xml, budget)
}

pub(super) fn validated_comment_ids<'a>(
    comments: &'a [DocxComment],
    budget: &Budget<'_>,
) -> Result<HashSet<&'a str>> {
    let mut comment_ids = HashSet::with_capacity(comments.len());
    for comment in comments {
        budget.checkpoint()?;
        if !comment_ids.insert(comment.id.as_str()) {
            anyhow::bail!(
                "extract_word: duplicate comment id '{}' in word/comments.xml",
                comment.id
            );
        }
    }
    Ok(comment_ids)
}

pub(super) fn attach_comment_anchors(
    comments: &mut [DocxComment],
    anchors: &HashMap<String, String>,
    budget: &Budget<'_>,
) -> Result<()> {
    for comment in comments {
        budget.checkpoint()?;
        if let Some(text) = anchors.get(&comment.id) {
            comment.anchored_text = Some(text.trim().to_string());
        }
    }
    Ok(())
}

fn parse_comments<R: BufRead>(xml: R, budget: &mut Budget<'_>) -> Result<Vec<DocxComment>> {
    use quick_xml::events::Event;

    let mut comments = Vec::new();
    let mut reader = quick_xml::Reader::from_reader(xml);
    reader.config_mut().check_comments = true;
    let mut buf = Vec::new();
    let mut current = None;
    let mut in_text = false;
    let mut document = XmlDocument::new("word/comments.xml");
    loop {
        budget.charge_item("DOCX comment XML events")?;
        let event = reader
            .read_event_into(&mut buf)
            .map_err(|error| anyhow::anyhow!("extract_word: invalid word/comments.xml: {error}"))?;
        document.observe(&event, budget)?;
        let is_empty = matches!(&event, Event::Empty(_));
        match event {
            Event::Start(ref event) | Event::Empty(ref event) => {
                let name = String::from_utf8_lossy(event.name().as_ref()).to_string();
                if name == "w:comment" {
                    if current.is_some() {
                        anyhow::bail!(
                            "extract_word: nested comments are invalid in word/comments.xml"
                        );
                    }
                    let comment = parse_comment(event, budget)?;
                    if is_empty {
                        budget.charge_item("DOCX comments")?;
                        comments.push(comment);
                    } else {
                        current = Some(comment);
                    }
                } else if name == "w:t" {
                    in_text = current.is_some() && !is_empty;
                }
            }
            Event::Text(ref event) if in_text => {
                if let Some(comment) = current.as_mut() {
                    if !comment.text.is_empty() {
                        budget.charge_output(1, "DOCX comment text")?;
                        comment.text.push(' ');
                    }
                    budget.charge_output(event.len() as u64, "DOCX comment text")?;
                    comment
                        .text
                        .push_str(&String::from_utf8_lossy(event.as_ref()));
                }
            }
            Event::End(ref event) => {
                let name = String::from_utf8_lossy(event.name().as_ref()).to_string();
                if name == "w:t" {
                    in_text = false;
                } else if name == "w:comment" {
                    let Some(comment) = current.take() else {
                        anyhow::bail!("extract_word: unmatched comment end in word/comments.xml");
                    };
                    budget.charge_item("DOCX comments")?;
                    comments.push(comment);
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
    Ok(comments)
}

fn parse_comment(
    event: &quick_xml::events::BytesStart<'_>,
    budget: &mut Budget<'_>,
) -> Result<DocxComment> {
    let mut comment = DocxComment::default();
    visit_attributes(event, "word/comments.xml", budget, |key, raw, budget| {
        let value = String::from_utf8_lossy(raw).to_string();
        match key {
            b"w:id" => {
                budget.charge_output(value.len() as u64, "DOCX comment id")?;
                comment.id = value;
            }
            b"w:author" => {
                budget.charge_output(value.len() as u64, "DOCX comment author")?;
                comment.author = Some(value);
            }
            b"w:initials" => {
                budget.charge_output(value.len() as u64, "DOCX comment initials")?;
                comment.initials = Some(value);
            }
            b"w:date" => {
                budget.charge_output(value.len() as u64, "DOCX comment date")?;
                comment.date = Some(value);
            }
            _ => {}
        }
        Ok(())
    })?;
    Ok(comment)
}

#[cfg(test)]
#[path = "comments/tests.rs"]
mod tests;
