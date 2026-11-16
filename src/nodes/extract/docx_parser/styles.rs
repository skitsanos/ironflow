//! Toggle run properties (`w:b`, `w:i`, `w:strike`) resolved through
//! `word/styles.xml`.
//!
//! A toggle is rolled up along each `w:basedOn` chain. The paragraph-style
//! and character-style results are then combined by exclusive or, a toggle
//! that the document defaults turn on stays on, and a toggle set directly on
//! the run replaces all of that. Table styles are not resolved.

use std::collections::HashMap;
use std::io::BufRead;

use anyhow::Result;
use quick_xml::events::{BytesStart, Event};

use super::super::resource::Budget;
use super::xml::{XmlDocument, attribute_value, visit_attributes};

const PART: &str = "word/styles.xml";
const TOGGLES: [&str; 3] = ["w:b", "w:i", "w:strike"];

/// Bold, italic and strikethrough, in that order.
pub(in crate::nodes::extract) type Flags = [bool; 3];
/// The same properties where each may be unset.
pub(super) type Direct = [Option<bool>; 3];

/// Index of a toggle property element in [`Flags`] and [`Direct`].
pub(super) fn toggle_index(name: &str) -> Option<usize> {
    TOGGLES.iter().position(|toggle| *toggle == name)
}

/// The on/off value of a toggle element: on unless `w:val` is `0`, `false`
/// or `off`. An absent `w:val` means on.
pub(super) fn on_off(
    event: &BytesStart<'_>,
    part: &str,
    version: quick_xml::XmlVersion,
    budget: &mut Budget<'_>,
) -> Result<bool> {
    let mut on = true;
    visit_attributes(event, part, version, budget, |key, value, _| {
        if key == b"w:val" {
            on = !matches!(value, b"0" | b"false" | b"off");
        }
        Ok(())
    })?;
    Ok(on)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Paragraph,
    Character,
}

struct Style {
    kind: Kind,
    based_on: Option<String>,
    toggles: Direct,
}

/// Rolled-up toggle state of every paragraph and character style.
#[derive(Default)]
pub(in crate::nodes::extract) struct StyleSheet {
    defaults: Flags,
    styles: HashMap<String, (Kind, Flags)>,
    default_paragraph: Flags,
    default_character: Flags,
}

impl StyleSheet {
    /// Effective toggles of a run. An unknown or absent style reference
    /// falls back to the default style of its kind.
    pub(super) fn resolve(
        &self,
        paragraph_style: Option<&str>,
        run_style: Option<&str>,
        direct: Direct,
    ) -> Flags {
        let paragraph = self.lookup(paragraph_style, Kind::Paragraph);
        let character = self.lookup(run_style, Kind::Character);
        std::array::from_fn(|index| {
            direct[index].unwrap_or(self.defaults[index] || (paragraph[index] ^ character[index]))
        })
    }

    fn lookup(&self, id: Option<&str>, kind: Kind) -> Flags {
        id.and_then(|id| self.styles.get(id))
            .filter(|(found, _)| *found == kind)
            .map(|(_, flags)| *flags)
            .unwrap_or(match kind {
                Kind::Paragraph => self.default_paragraph,
                Kind::Character => self.default_character,
            })
    }
}

/// Where the walk is inside `w:styles`. Depths are element nesting levels.
#[derive(Default)]
struct Walk {
    depth: u64,
    /// Depth of the open `w:style` or `w:rPrDefault`.
    container: Option<u64>,
    /// The container is `w:rPrDefault`, not a style.
    in_defaults: bool,
    /// Depth of the `w:rPr` directly inside the container.
    properties: Option<u64>,
    /// The open `w:style`: its id, whether it is a default, and its content.
    style: Option<(String, bool, Style)>,
    defaults: Direct,
    raw: HashMap<String, Style>,
    default_ids: [Option<String>; 2],
}

/// Parse `word/styles.xml` into rolled-up toggle state.
pub(in crate::nodes::extract) fn parse_styles<R: BufRead>(
    xml: R,
    budget: &mut Budget<'_>,
) -> Result<StyleSheet> {
    let mut reader = quick_xml::Reader::from_reader(xml);
    reader.config_mut().check_comments = true;
    let mut buf = Vec::new();
    let mut document = XmlDocument::new(PART);
    let mut walk = Walk::default();
    loop {
        budget.charge_item("DOCX styles XML events")?;
        let event = reader
            .read_event_into(&mut buf)
            .map_err(|error| anyhow::anyhow!("extract_word: invalid {PART}: {error}"))?;
        match document.decode(event, budget)? {
            Event::Start(ref event) => {
                walk.depth += 1;
                walk.open(event, document.version(), budget)?;
            }
            Event::Empty(ref event) => {
                walk.depth += 1;
                walk.open(event, document.version(), budget)?;
                walk.close(budget)?;
            }
            Event::End(_) => walk.close(budget)?,
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
    walk.finish(budget)
}

impl Walk {
    fn open(
        &mut self,
        event: &BytesStart<'_>,
        version: quick_xml::XmlVersion,
        budget: &mut Budget<'_>,
    ) -> Result<()> {
        let name = event.name();
        let name = name.as_ref();
        let parent = self.depth.checked_sub(1);
        match name {
            "w:style" | "w:rPrDefault" if self.container.is_none() => {
                self.container = Some(self.depth);
                self.in_defaults = name == "w:rPrDefault";
                if !self.in_defaults {
                    self.style = read_style(event, version, budget)?;
                }
            }
            "w:basedOn" if self.container == parent => {
                if let Some((_, _, style)) = self.style.as_mut() {
                    style.based_on = attribute_value(event, b"w:val", PART, version, budget)?;
                    let length = style.based_on.as_ref().map_or(0, String::len);
                    budget.charge_output(length as u64, "DOCX retained style references")?;
                }
            }
            "w:rPr" if self.container == parent => self.properties = Some(self.depth),
            _ if self.properties.is_some() && self.properties == parent => {
                if let Some(index) = toggle_index(name) {
                    let value = Some(on_off(event, PART, version, budget)?);
                    // A style of a kind that is not retained has no `style`.
                    if self.in_defaults {
                        self.defaults[index] = value;
                    } else if let Some((_, _, style)) = self.style.as_mut() {
                        style.toggles[index] = value;
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn close(&mut self, budget: &mut Budget<'_>) -> Result<()> {
        if self.properties == Some(self.depth) {
            self.properties = None;
        }
        if self.container == Some(self.depth) {
            self.container = None;
            if let Some((id, is_default, style)) = self.style.take() {
                budget.charge_item("DOCX styles")?;
                if is_default {
                    self.default_ids[style.kind as usize] = Some(id.clone());
                }
                self.raw.insert(id, style);
            }
        }
        self.depth = self.depth.saturating_sub(1);
        Ok(())
    }

    fn finish(self, budget: &mut Budget<'_>) -> Result<StyleSheet> {
        let mut styles = HashMap::with_capacity(self.raw.len());
        for (id, style) in &self.raw {
            styles.insert(
                id.clone(),
                (style.kind, rolled_up(&self.raw, style, budget)?),
            );
        }
        let default = |kind: Kind| {
            self.default_ids[kind as usize]
                .as_ref()
                .and_then(|id| styles.get(id))
                .map(|(_, flags)| *flags)
                .unwrap_or_default()
        };
        Ok(StyleSheet {
            defaults: self.defaults.map(|value| value.unwrap_or(false)),
            default_paragraph: default(Kind::Paragraph),
            default_character: default(Kind::Character),
            styles,
        })
    }
}

/// A `w:style` of a kind whose run properties apply to runs. `w:type`
/// defaults to `paragraph`; table and numbering styles are not retained.
fn read_style(
    event: &BytesStart<'_>,
    version: quick_xml::XmlVersion,
    budget: &mut Budget<'_>,
) -> Result<Option<(String, bool, Style)>> {
    let mut id = None;
    let mut kind = Some(Kind::Paragraph);
    let mut is_default = false;
    visit_attributes(event, PART, version, budget, |key, value, budget| {
        match key {
            b"w:styleId" => {
                budget.charge_output(value.len() as u64, "DOCX retained style ids")?;
                id = Some(String::from_utf8_lossy(value).to_string());
            }
            b"w:type" => {
                kind = match value {
                    b"paragraph" => Some(Kind::Paragraph),
                    b"character" => Some(Kind::Character),
                    _ => None,
                };
            }
            b"w:default" => is_default = matches!(value, b"1" | b"true" | b"on"),
            _ => {}
        }
        Ok(())
    })?;
    Ok(id.zip(kind).map(|(id, kind)| {
        let style = Style {
            kind,
            based_on: None,
            toggles: Direct::default(),
        };
        (id, is_default, style)
    }))
}

/// The nearest value of each toggle along the `w:basedOn` chain. A chain
/// stops at a missing style, a style of another kind, or after as many steps
/// as there are styles, which ends a cycle.
fn rolled_up(
    raw: &HashMap<String, Style>,
    style: &Style,
    budget: &mut Budget<'_>,
) -> Result<Flags> {
    let mut found = style.toggles;
    let mut next = style.based_on.as_deref();
    let mut steps = 0_usize;
    while found.contains(&None)
        && steps < raw.len()
        && let Some(parent) = next.and_then(|id| raw.get(id))
        && parent.kind == style.kind
    {
        budget.charge_item("DOCX style inheritance steps")?;
        steps += 1;
        for (value, inherited) in found.iter_mut().zip(parent.toggles) {
            *value = value.or(inherited);
        }
        next = parent.based_on.as_deref();
    }
    Ok(found.map(|value| value.unwrap_or(false)))
}
