//! Locates the core-properties part through the package relationships.
//!
//! Desktop Office writes `docProps/core.xml`, but the part name is not fixed:
//! Office for the web writes `package/services/metadata/core-properties/<id>.psmdcp`
//! and names it only in `_rels/.rels`. The fixed path stays as the fallback
//! for packages without that relationship.

use anyhow::Result;

use super::Archive;
use super::part_paths::resolve_part;
use super::relationships::{Labels, parse_relationships};
use crate::nodes::extract::resource::Budget;
use crate::util::execution::ExecutionControl;

pub(in crate::nodes::extract) const FALLBACK_PART: &str = "docProps/core.xml";
const PACKAGE_RELATIONSHIPS: &str = "_rels/.rels";
const CORE_PROPERTIES_TYPES: [&str; 2] = [
    "http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties",
    // Written by early Office 2007 builds and accepted by Office.
    "http://schemas.openxmlformats.org/officedocument/2006/relationships/metadata/core-properties",
];

/// Returns the archive part that holds the core properties: the target of
/// the package-level core-properties relationship, or `docProps/core.xml`
/// when the package has no `_rels/.rels` or no such relationship.
pub(in crate::nodes::extract) fn core_properties_part(
    archive: &mut Archive,
    labels: &Labels,
    budget: &mut Budget<'_>,
    execution: &ExecutionControl,
) -> Result<String> {
    let operation = labels.operation;
    let labels = Labels {
        noun: "package relationship",
        ..*labels
    };
    let relationships = archive
        .with_optional_xml(PACKAGE_RELATIONSHIPS, execution, |reader| {
            parse_relationships(reader, &labels, is_core_properties, budget)
        })?
        .unwrap_or_default();
    let mut found = None;
    for relationship in relationships.into_values() {
        budget.checkpoint()?;
        if !relationship.kind {
            continue;
        }
        anyhow::ensure!(
            found.is_none(),
            "{operation}: multiple core-properties relationships in {PACKAGE_RELATIONSHIPS}"
        );
        anyhow::ensure!(
            !relationship.external,
            "{operation}: external core-properties relationship is not supported"
        );
        found = Some(relationship.target);
    }
    let Some(target) = found else {
        return Ok(FALLBACK_PART.to_string());
    };
    // Package relationships resolve against the package root.
    let part = resolve_part(operation, "", &target)?;
    budget.charge_output(part.len() as u64, "retained core-properties part name")?;
    Ok(part)
}

fn is_core_properties(value: &str) -> bool {
    CORE_PROPERTIES_TYPES.contains(&value)
}
