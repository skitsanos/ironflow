use std::collections::HashMap;
use std::io::BufRead;

use anyhow::{Context, Result};
use quick_xml::events::{BytesStart, Event};

use super::PptxComment;
use crate::nodes::extract::ooxml::Archive;
use crate::nodes::extract::resource::Budget;
use crate::util::execution::ExecutionControl;

type Author = (Option<String>, Option<String>);

pub(in crate::nodes::extract) fn extract_pptx_comments(
    archive: &mut Archive,
    budget: &mut Budget<'_>,
    execution: &ExecutionControl,
) -> Result<Vec<PptxComment>> {
    let authors = read_authors(archive, budget, execution)?;
    let mut parts = archive
        .entry_names("ppt/comments/comment", ".xml", execution)?
        .into_iter()
        .map(|name| comment_part(name, budget))
        .collect::<Result<Vec<_>>>()?;
    parts.sort_by_key(|(index, _)| *index);

    let mut comments = Vec::new();
    for (slide_index, name) in parts {
        budget.checkpoint()?;
        let parsed = archive.with_required_xml(&name, execution, |reader| {
            parse_comments(reader, slide_index, &authors, budget)
        })?;
        comments.extend(parsed);
    }
    Ok(comments)
}

fn read_authors(
    archive: &mut Archive,
    budget: &mut Budget<'_>,
    execution: &ExecutionControl,
) -> Result<HashMap<String, Author>> {
    Ok(archive
        .with_optional_xml("ppt/commentAuthors.xml", execution, |reader| {
            parse_authors(reader, budget)
        })?
        .unwrap_or_default())
}

fn parse_authors<R: BufRead>(xml: R, budget: &mut Budget<'_>) -> Result<HashMap<String, Author>> {
    let mut authors = HashMap::new();
    let mut reader = quick_xml::Reader::from_reader(xml);
    let mut buffer = Vec::new();
    let mut saw_element = false;
    let mut depth = 0_u64;
    loop {
        budget.checkpoint()?;
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Start(event)) => {
                saw_element = true;
                depth = depth.saturating_add(1);
                budget.charge_item("PPTX comment-author XML events")?;
                if local_name(event.name().as_ref()) == b"cmAuthor"
                    && let Some((id, author)) = parse_author(&event)?
                {
                    budget.charge_item("PPTX comment authors")?;
                    authors.insert(id, author);
                }
            }
            Ok(Event::Empty(event)) => {
                saw_element = true;
                budget.charge_item("PPTX comment-author XML events")?;
                if local_name(event.name().as_ref()) == b"cmAuthor"
                    && let Some((id, author)) = parse_author(&event)?
                {
                    budget.charge_item("PPTX comment authors")?;
                    authors.insert(id, author);
                }
            }
            Ok(Event::End(_)) => {
                budget.charge_item("PPTX comment-author XML events")?;
                depth = depth.checked_sub(1).ok_or_else(|| {
                    anyhow::anyhow!("extract_pptx: unmatched closing element in comment authors")
                })?;
            }
            Ok(Event::Eof) => break,
            Ok(_) => budget.charge_item("PPTX comment-author XML events")?,
            Err(error) => {
                anyhow::bail!("extract_pptx: invalid XML in comment authors: {error}")
            }
        }
        buffer.clear();
    }
    if !saw_element || depth != 0 {
        anyhow::bail!("extract_pptx: incomplete XML in comment authors part");
    }
    Ok(authors)
}

fn parse_author(event: &BytesStart<'_>) -> Result<Option<(String, Author)>> {
    let mut id = None;
    let mut name = None;
    let mut initials = None;
    for attribute in event.attributes() {
        let attribute = attribute.context("extract_pptx: invalid comment-author attribute")?;
        let value = String::from_utf8_lossy(&attribute.value).to_string();
        match attribute.key.as_ref() {
            b"id" => id = Some(value),
            b"name" => name = Some(value),
            b"initials" => initials = Some(value),
            _ => {}
        }
    }
    let Some(id) = id.filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    Ok(Some((id, (name, initials))))
}

fn comment_part(name: String, budget: &mut Budget<'_>) -> Result<(u32, String)> {
    budget.charge_item("PPTX comment archive parts")?;
    let suffix = name
        .strip_prefix("ppt/comments/comment")
        .and_then(|value| value.strip_suffix(".xml"))
        .ok_or_else(|| anyhow::anyhow!("extract_pptx: invalid comment archive part: {name}"))?;
    let index = suffix
        .parse::<u32>()
        .with_context(|| format!("extract_pptx: invalid comment slide number: {name}"))?;
    if index == 0 {
        anyhow::bail!("extract_pptx: comment slide numbers must start at one: {name}");
    }
    Ok((index, name))
}

fn parse_comments<R: BufRead>(
    xml: R,
    slide_index: u32,
    authors: &HashMap<String, Author>,
    budget: &mut Budget<'_>,
) -> Result<Vec<PptxComment>> {
    let mut comments = Vec::new();
    let mut reader = quick_xml::Reader::from_reader(xml);
    let mut buffer = Vec::new();
    let mut current = None;
    let mut in_text = false;
    let mut saw_element = false;
    let mut depth = 0_u64;
    loop {
        budget.checkpoint()?;
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Start(event)) => {
                saw_element = true;
                depth = depth.saturating_add(1);
                budget.charge_item("PPTX comment XML events")?;
                match local_name(event.name().as_ref()) {
                    b"cm" => {
                        if current.is_some() {
                            anyhow::bail!("extract_pptx: nested comments are not supported");
                        }
                        current = Some(parse_comment(&event, slide_index, authors, budget)?);
                    }
                    b"text" => in_text = current.is_some(),
                    _ => {}
                }
            }
            Ok(Event::Empty(event)) => {
                saw_element = true;
                budget.charge_item("PPTX comment XML events")?;
                if local_name(event.name().as_ref()) == b"cm" {
                    let comment = parse_comment(&event, slide_index, authors, budget)?;
                    budget.charge_item("PPTX comments")?;
                    comments.push(comment);
                }
            }
            Ok(Event::Text(event)) if in_text => {
                budget.charge_item("PPTX comment XML events")?;
                if let Some(comment) = current.as_mut() {
                    budget.charge_output(event.len() as u64, "PPTX retained comment text")?;
                    comment
                        .text
                        .push_str(&String::from_utf8_lossy(event.as_ref()));
                }
            }
            Ok(Event::End(event)) => {
                budget.charge_item("PPTX comment XML events")?;
                depth = depth.checked_sub(1).ok_or_else(|| {
                    anyhow::anyhow!("extract_pptx: unmatched closing element in comments")
                })?;
                match local_name(event.name().as_ref()) {
                    b"text" => in_text = false,
                    b"cm" => {
                        if let Some(comment) = current.take() {
                            budget.charge_item("PPTX comments")?;
                            comments.push(comment);
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Ok(_) => budget.charge_item("PPTX comment XML events")?,
            Err(error) => anyhow::bail!("extract_pptx: invalid XML in comments: {error}"),
        }
        buffer.clear();
    }
    if !saw_element || current.is_some() || depth != 0 {
        anyhow::bail!("extract_pptx: incomplete XML in comments part");
    }
    Ok(comments)
}

fn parse_comment(
    event: &BytesStart<'_>,
    slide_index: u32,
    authors: &HashMap<String, Author>,
    budget: &mut Budget<'_>,
) -> Result<PptxComment> {
    let mut comment = PptxComment {
        slide_index,
        ..Default::default()
    };
    for attribute in event.attributes() {
        let attribute = attribute.context("extract_pptx: invalid comment attribute")?;
        let value = String::from_utf8_lossy(&attribute.value).to_string();
        match attribute.key.as_ref() {
            b"authorId" => {
                if let Some((name, initials)) = authors.get(&value) {
                    if let Some(name) = name {
                        budget.charge_output(name.len() as u64, "PPTX retained comment authors")?;
                    }
                    if let Some(initials) = initials {
                        budget.charge_output(
                            initials.len() as u64,
                            "PPTX retained comment initials",
                        )?;
                    }
                    comment.author = name.clone();
                    comment.initials = initials.clone();
                }
                budget.charge_output(value.len() as u64, "PPTX retained comment attributes")?;
                comment.author_id = Some(value);
            }
            b"dt" => {
                budget.charge_output(value.len() as u64, "PPTX retained comment attributes")?;
                comment.date = Some(value);
            }
            b"idx" => {
                budget.charge_output(value.len() as u64, "PPTX retained comment attributes")?;
                comment.idx = Some(value);
            }
            _ => {}
        }
    }
    Ok(comment)
}

fn local_name(name: &[u8]) -> &[u8] {
    name.rsplit(|byte| *byte == b':').next().unwrap_or(name)
}
