//! Bullets a placeholder inherits from its slide layout and slide master.
//!
//! A placeholder paragraph that declares no bullet takes the first
//! declaration for its level from: the matching placeholder of the slide
//! layout, the matching placeholder of the slide master, and the master's
//! `p:txStyles` (title, body or other). The layout placeholder is the one
//! with the same `idx`, or else the first of the same type; the master
//! placeholder is the first of the same role. Each layout and master part is
//! read once per call.

use std::collections::HashMap;
use std::rc::Rc;

use anyhow::Result;

use super::graph::{SlidePart, resolve_target};
use super::list_style;
use super::relationships::{Kind, Relationships, parse_pptx_rels};
use crate::nodes::extract::ooxml::Archive;
use crate::nodes::extract::ooxml::part_paths::relationships_part;
use crate::nodes::extract::resource::Budget;
use crate::util::execution::ExecutionControl;

mod sheet;

use sheet::{Role, Sheet, parse_sheet};

/// What the layout and master say about one paragraph level.
pub(super) enum Resolved {
    Bullet,
    Plain,
    /// The slide has no readable layout and master.
    Unknown,
}

/// The layout and master of one slide. Both are present or neither is.
#[derive(Default)]
pub(super) struct Inherited(Option<(Rc<Sheet>, Rc<Sheet>)>);

impl Inherited {
    /// The inherited bullet of a placeholder (`type` and `idx` of its
    /// `p:ph`) at a 0-based paragraph level.
    pub(super) fn bullet(&self, kind: Option<&str>, idx: Option<&str>, level: u32) -> Resolved {
        let Some((layout, master)) = &self.0 else {
            return Resolved::Unknown;
        };
        let on_layout = layout.placeholder(kind, idx);
        // A slide placeholder without a type has the type of its layout's.
        let role = Role::of(kind.or(on_layout.and_then(|found| found.kind.as_deref())));
        let declared = on_layout
            .and_then(|found| list_style::at(&found.levels, level))
            .or_else(|| {
                let found = master.placeholder_of(role)?;
                list_style::at(&found.levels, level)
            })
            .or_else(|| list_style::at(master.text_style(role), level));
        match declared {
            Some(true) => Resolved::Bullet,
            _ => Resolved::Plain,
        }
    }
}

/// Layout and master parts read during one extraction, by part name. A part
/// that is absent is remembered as absent.
#[derive(Default)]
pub(super) struct Sheets {
    layouts: HashMap<String, Inherited>,
    masters: HashMap<String, Option<Rc<Sheet>>>,
}

impl Sheets {
    /// The layout and master a slide inherits from. A slide without exactly
    /// one internal layout relationship, or whose layout or master part is
    /// absent, inherits nothing.
    pub(super) fn of_slide(
        &mut self,
        archive: &mut Archive,
        slide: &SlidePart,
        budget: &mut Budget<'_>,
        execution: &ExecutionControl,
    ) -> Result<&Inherited> {
        let Some(layout) = only_target(&slide.part, &slide.relationships, Kind::Layout, budget)?
        else {
            return Ok(self.layouts.entry(String::new()).or_default());
        };
        if !self.layouts.contains_key(&layout) {
            budget.charge_output(layout.len() as u64, "PPTX retained layout part names")?;
            let inherited = self.read_layout(archive, &layout, budget, execution)?;
            self.layouts.insert(layout.clone(), inherited);
        }
        Ok(&self.layouts[&layout])
    }

    fn read_layout(
        &mut self,
        archive: &mut Archive,
        layout: &str,
        budget: &mut Budget<'_>,
        execution: &ExecutionControl,
    ) -> Result<Inherited> {
        budget.charge_item("PPTX slide layouts")?;
        let Some(sheet) = read_sheet(archive, layout, budget, execution)? else {
            return Ok(Inherited(None));
        };
        let relationships = archive
            .with_optional_xml(&relationships_part(layout), execution, |reader| {
                parse_pptx_rels(reader, budget)
            })?
            .unwrap_or_default();
        let Some(master) = only_target(layout, &relationships, Kind::Master, budget)? else {
            return Ok(Inherited(None));
        };
        if !self.masters.contains_key(&master) {
            budget.charge_item("PPTX slide masters")?;
            budget.charge_output(master.len() as u64, "PPTX retained master part names")?;
            let sheet = read_sheet(archive, &master, budget, execution)?;
            self.masters.insert(master.clone(), sheet);
        }
        Ok(Inherited(
            self.masters[&master].clone().map(|master| (sheet, master)),
        ))
    }
}

fn read_sheet(
    archive: &mut Archive,
    part: &str,
    budget: &mut Budget<'_>,
    execution: &ExecutionControl,
) -> Result<Option<Rc<Sheet>>> {
    let sheet = archive.with_optional_xml(part, execution, |reader| parse_sheet(reader, budget))?;
    Ok(sheet.map(Rc::new))
}

/// The part behind the only internal relationship of a kind. None or
/// several such relationships resolve to nothing, so the choice never
/// depends on the order of a map.
fn only_target(
    source: &str,
    relationships: &Relationships,
    kind: Kind,
    budget: &mut Budget<'_>,
) -> Result<Option<String>> {
    let mut found = None;
    for relationship in relationships.values() {
        budget.checkpoint()?;
        if relationship.kind != kind || relationship.external {
            continue;
        }
        if found.is_some() {
            return Ok(None);
        }
        found = Some(relationship);
    }
    found
        .map(|relationship| resolve_target(source, &relationship.target, budget))
        .transpose()
}
