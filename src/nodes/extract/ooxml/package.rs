//! Locates the main document and core-properties parts through the package
//! relationships in `_rels/.rels`.
//!
//! Part names are not fixed: Office for the web writes core properties to
//! `package/services/metadata/core-properties/<id>.psmdcp`, and a package may
//! store its main part anywhere. Each part is named only by its package
//! relationship; the conventional paths stay as fallbacks for packages
//! without one.

use anyhow::Result;
use quick_xml::name::ResolveResult;

use super::Archive;
use super::part_paths::resolve_part;
use super::relationships::{Labels, Relationships, office_document_type, parse_relationships};
use crate::nodes::extract::resource::Budget;
use crate::util::execution::ExecutionControl;

pub(in crate::nodes::extract) const FALLBACK_PART: &str = "docProps/core.xml";
const PACKAGE_RELATIONSHIPS: &str = "_rels/.rels";
const CORE_PROPERTIES_TYPES: [&str; 2] = [
    "http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties",
    // Written by early Office 2007 builds and accepted by Office.
    "http://schemas.openxmlformats.org/officedocument/2006/relationships/metadata/core-properties",
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    OfficeDocument,
    CoreProperties,
    Other,
}

fn classify(value: &str) -> Kind {
    if CORE_PROPERTIES_TYPES.contains(&value) {
        Kind::CoreProperties
    } else if office_document_type(value) == Some("officeDocument") {
        Kind::OfficeDocument
    } else {
        Kind::Other
    }
}

/// Parts named by the package relationships.
pub(in crate::nodes::extract) struct PackageParts {
    /// The main document part, or `None` when the package names none.
    pub(in crate::nodes::extract) main: Option<String>,
    /// The core-properties part, or `docProps/core.xml` when none is named.
    pub(in crate::nodes::extract) core_properties: String,
}

/// Reads `_rels/.rels` once. A missing part names nothing; a malformed one,
/// or an external, escaping or duplicated main or core-properties
/// relationship, fails the step.
pub(in crate::nodes::extract) fn package_parts(
    archive: &mut Archive,
    labels: &Labels,
    budget: &mut Budget<'_>,
    execution: &ExecutionControl,
) -> Result<PackageParts> {
    let labels = Labels {
        noun: "package relationship",
        ..*labels
    };
    let relationships = archive
        .with_optional_xml(PACKAGE_RELATIONSHIPS, execution, |reader| {
            parse_relationships(reader, &labels, classify, budget)
        })?
        .unwrap_or_default();
    let main = single_part(
        &relationships,
        Kind::OfficeDocument,
        "main document",
        &labels,
        budget,
    )?;
    let core = single_part(
        &relationships,
        Kind::CoreProperties,
        "core-properties",
        &labels,
        budget,
    )?;
    Ok(PackageParts {
        main,
        core_properties: core.unwrap_or_else(|| FALLBACK_PART.to_string()),
    })
}

/// Returns the archive part that holds the core properties: the target of
/// the package-level core-properties relationship, or `docProps/core.xml`
/// when the package has no `_rels/.rels` or no such relationship.
pub(in crate::nodes::extract) fn core_properties_part(
    archive: &mut Archive,
    labels: &Labels,
    budget: &mut Budget<'_>,
    execution: &ExecutionControl,
) -> Result<String> {
    Ok(package_parts(archive, labels, budget, execution)?.core_properties)
}

fn single_part(
    relationships: &Relationships<Kind>,
    kind: Kind,
    name: &str,
    labels: &Labels,
    budget: &mut Budget<'_>,
) -> Result<Option<String>> {
    let operation = labels.operation;
    let mut found = None;
    for relationship in relationships.values() {
        budget.checkpoint()?;
        if relationship.kind != kind {
            continue;
        }
        anyhow::ensure!(
            found.is_none(),
            "{operation}: multiple {name} relationships in {PACKAGE_RELATIONSHIPS}"
        );
        anyhow::ensure!(
            !relationship.external,
            "{operation}: external {name} relationship is not supported"
        );
        found = Some(relationship.target.as_str());
    }
    let Some(target) = found else {
        return Ok(None);
    };
    // Package relationships resolve against the package root.
    let part = resolve_part(operation, "", target)?;
    budget.charge_output(part.len() as u64, "retained package part names")?;
    Ok(Some(part))
}

const DC: &str = "http://purl.org/dc/elements/1.1/";
const DC_TERMS: &str = "http://purl.org/dc/terms/";
const CORE: &str = "http://schemas.openxmlformats.org/package/2006/metadata/core-properties";

/// Maps a core-properties element to its output key by namespace and local
/// name, never by prefix: Office for the web writes `lastModifiedBy` in the
/// default namespace without a `cp:` prefix.
pub(in crate::nodes::extract) fn property_key(
    namespace: ResolveResult<'_>,
    local: &str,
) -> Option<&'static str> {
    let ResolveResult::Bound(namespace) = namespace else {
        return None;
    };
    match (namespace.as_ref(), local) {
        (DC, "title") => Some("title"),
        (DC, "creator") => Some("author"),
        (DC, "subject") => Some("subject"),
        (DC, "description") => Some("description"),
        (CORE, "keywords") => Some("keywords"),
        (CORE, "lastModifiedBy") => Some("last_modified_by"),
        (CORE, "revision") => Some("revision"),
        (CORE, "category") => Some("category"),
        (DC_TERMS, "created") => Some("created"),
        (DC_TERMS, "modified") => Some("modified"),
        _ => None,
    }
}
