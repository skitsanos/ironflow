//! The list styles of one slide layout or slide master part.

use std::io::BufRead;

use anyhow::{Context, Result};
use quick_xml::events::{BytesStart, Event};

use super::super::list_style::{Levels, ListStyle};
use crate::nodes::extract::resource::Budget;
use crate::util::xml::{Decoder, attribute_value};

/// Which master placeholder and text style a placeholder type inherits from.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Role {
    Title,
    Body,
    Date,
    Footer,
    Header,
    SlideNumber,
}

impl Role {
    /// The role of a `p:ph` type. An absent type is `obj`, which like every
    /// content type (subtitle, chart, table, picture) inherits from the body.
    pub(super) fn of(kind: Option<&str>) -> Self {
        match kind {
            Some("title" | "ctrTitle") => Self::Title,
            Some("dt") => Self::Date,
            Some("ftr") => Self::Footer,
            Some("hdr") => Self::Header,
            Some("sldNum") => Self::SlideNumber,
            _ => Self::Body,
        }
    }
}

/// A placeholder shape of the part and the list style of its text body.
pub(super) struct Placeholder {
    pub(super) kind: Option<String>,
    idx: Option<String>,
    pub(super) levels: Levels,
}

#[derive(Default)]
pub(super) struct Sheet {
    placeholders: Vec<Placeholder>,
    /// `p:titleStyle`, `p:bodyStyle` and `p:otherStyle` of a master.
    text_styles: [Levels; 3],
}

impl Sheet {
    /// The placeholder a slide placeholder inherits from: the one with the
    /// same `idx` (absent is `0`), or else the first of the same type.
    pub(super) fn placeholder(
        &self,
        kind: Option<&str>,
        idx: Option<&str>,
    ) -> Option<&Placeholder> {
        let idx = idx.unwrap_or("0");
        self.placeholders
            .iter()
            .find(|found| found.idx.as_deref().unwrap_or("0") == idx)
            .or_else(|| {
                let kind = kind?;
                self.placeholders
                    .iter()
                    .find(|found| found.kind.as_deref() == Some(kind))
            })
    }

    /// The first placeholder of a role, as a master declares one per role.
    pub(super) fn placeholder_of(&self, role: Role) -> Option<&Placeholder> {
        self.placeholders
            .iter()
            .find(|found| Role::of(found.kind.as_deref()) == role)
    }

    pub(super) fn text_style(&self, role: Role) -> &Levels {
        &self.text_styles[text_style_index(role)]
    }
}

fn text_style_index(role: Role) -> usize {
    match role {
        Role::Title => 0,
        Role::Body => 1,
        _ => 2,
    }
}

/// Where the walk is: inside a shape, and inside which list style.
#[derive(Default)]
struct Walk {
    sheet: Sheet,
    in_shape: bool,
    /// `type` and `idx` of the open shape's `p:ph`.
    placeholder: Option<(Option<String>, Option<String>)>,
    /// The open `a:lstStyle` of a shape, or the open text style by index.
    target: Option<Target>,
    style: ListStyle,
    shape_levels: Levels,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Target {
    Shape,
    TextStyle(usize),
}

pub(super) fn parse_sheet(xml: &mut dyn BufRead, budget: &mut Budget<'_>) -> Result<Sheet> {
    let mut reader = quick_xml::Reader::from_reader(xml);
    let mut decoder = Decoder::default();
    let mut buffer = Vec::new();
    let mut walk = Walk::default();
    loop {
        budget.charge_item("PPTX layout and master XML events")?;
        let event = reader
            .read_event_into(&mut buffer)
            .map_err(anyhow::Error::from)
            .and_then(|event| {
                decoder.decode(event, |bytes| {
                    budget.charge_output(bytes, "PPTX decoded layout and master text")
                })
            })
            .map_err(|error| {
                anyhow::anyhow!("extract_pptx: invalid XML in slide layout or master: {error}")
            })?;
        match event {
            Event::Start(event) => walk.open(&event, decoder.version, budget)?,
            Event::Empty(event) => {
                walk.open(&event, decoder.version, budget)?;
                walk.close(local_name(event.name().as_ref().as_bytes()), budget)?;
            }
            Event::End(event) => {
                walk.close(local_name(event.name().as_ref().as_bytes()), budget)?
            }
            Event::Eof => break,
            _ => {}
        }
        buffer.clear();
    }
    Ok(walk.sheet)
}

impl Walk {
    fn open(
        &mut self,
        event: &BytesStart<'_>,
        version: quick_xml::XmlVersion,
        budget: &mut Budget<'_>,
    ) -> Result<()> {
        let name = event.name();
        let name = local_name(name.as_ref().as_bytes());
        match name {
            b"sp" => {
                self.in_shape = true;
                self.placeholder = None;
                self.shape_levels = Levels::default();
            }
            b"ph" if self.in_shape => {
                self.placeholder = Some(placeholder_attributes(event, version, budget)?);
            }
            b"lstStyle" if self.in_shape => self.begin(Target::Shape),
            b"titleStyle" if !self.in_shape => self.begin(Target::TextStyle(0)),
            b"bodyStyle" if !self.in_shape => self.begin(Target::TextStyle(1)),
            b"otherStyle" if !self.in_shape => self.begin(Target::TextStyle(2)),
            _ if self.target.is_some() => self.style.open(name),
            _ => {}
        }
        Ok(())
    }

    fn begin(&mut self, target: Target) {
        self.target = Some(target);
        self.style = ListStyle::default();
    }

    fn close(&mut self, name: &[u8], budget: &mut Budget<'_>) -> Result<()> {
        match (name, self.target) {
            (b"lstStyle", Some(Target::Shape)) => {
                self.shape_levels = self.style.levels;
                self.target = None;
            }
            (b"titleStyle" | b"bodyStyle" | b"otherStyle", Some(Target::TextStyle(index))) => {
                self.sheet.text_styles[index] = self.style.levels;
                self.target = None;
            }
            (b"sp", _) => {
                self.in_shape = false;
                self.target = None;
                if let Some((kind, idx)) = self.placeholder.take() {
                    budget.charge_item("PPTX layout and master placeholders")?;
                    self.sheet.placeholders.push(Placeholder {
                        kind,
                        idx,
                        levels: self.shape_levels,
                    });
                }
            }
            _ => self.style.close(name),
        }
        Ok(())
    }
}

/// `type` and `idx` of a `p:ph`.
fn placeholder_attributes(
    event: &BytesStart<'_>,
    version: quick_xml::XmlVersion,
    budget: &mut Budget<'_>,
) -> Result<(Option<String>, Option<String>)> {
    let (mut kind, mut idx) = (None, None);
    for attribute in event.attributes() {
        let attribute = attribute.context("extract_pptx: invalid placeholder attribute")?;
        let target = match attribute.key.as_ref().as_bytes() {
            b"type" => &mut kind,
            b"idx" => &mut idx,
            _ => continue,
        };
        *target = Some(
            attribute_value(&attribute, version, |bytes| {
                budget.charge_output(bytes, "PPTX retained placeholder attributes")
            })?
            .into_owned(),
        );
    }
    Ok((kind, idx))
}

fn local_name(name: &[u8]) -> &[u8] {
    name.rsplit(|byte| *byte == b':').next().unwrap_or(name)
}
