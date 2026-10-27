use super::super::resource::Budget;
use anyhow::{Context as _, Result, bail, ensure};
use lopdf::{Dictionary, Document, Object, ObjectId};

const MAX_REFERENCES: usize = 32;

pub(super) fn object<'a>(
    doc: &'a Document,
    mut value: &'a Object,
    budget: &mut Budget<'_>,
) -> Result<&'a Object> {
    let mut visited = Vec::new();
    while let Object::Reference(id) = value {
        budget.charge_item("PDF property references")?;
        ensure!(!visited.contains(id), "PDF property reference cycle");
        ensure!(
            visited.len() < MAX_REFERENCES,
            "PDF property reference depth exceeds 32"
        );
        visited.push(*id);
        value = doc
            .objects
            .get(id)
            .context("missing PDF property reference")?;
    }
    Ok(value)
}

pub(super) fn properties<'a>(
    doc: &'a Document,
    page: ObjectId,
    value: &'a Object,
    budget: &mut Budget<'_>,
) -> Result<&'a Dictionary> {
    let value = object(doc, value, budget)?;
    let value = match value {
        Object::Name(name) => {
            let resources = resources(doc, page, budget)?;
            let properties = object(
                doc,
                resources
                    .get(b"Properties")
                    .context("missing PDF Properties resource dictionary")?,
                budget,
            )?
            .as_dict()
            .context("PDF Properties resource is not a dictionary")?;
            object(
                doc,
                properties
                    .get(name)
                    .context("missing named PDF marked-content property")?,
                budget,
            )?
        }
        value => value,
    };
    value
        .as_dict()
        .context("PDF marked-content property is not a dictionary")
}

fn resources<'a>(
    doc: &'a Document,
    page: ObjectId,
    budget: &mut Budget<'_>,
) -> Result<&'a Dictionary> {
    let mut current = page;
    let mut visited = Vec::new();
    loop {
        budget.charge_item("PDF resource ancestors")?;
        ensure!(!visited.contains(&current), "PDF resource parent cycle");
        ensure!(
            visited.len() < 64,
            "PDF resource inheritance depth exceeds 64"
        );
        visited.push(current);
        let page = doc
            .objects
            .get(&current)
            .context("missing PDF resource ancestor")?;
        let dictionary = object(doc, page, budget)?.as_dict()?;
        if let Ok(resources) = dictionary.get(b"Resources") {
            let resources = object(doc, resources, budget)?;
            if !matches!(resources, Object::Null) {
                return resources
                    .as_dict()
                    .context("PDF Resources is not a dictionary");
            }
        }
        let Ok(parent) = dictionary.get(b"Parent") else {
            bail!("missing PDF page resources");
        };
        current = parent
            .as_reference()
            .context("PDF resource parent is not a reference")?;
    }
}
