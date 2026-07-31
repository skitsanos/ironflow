use anyhow::Result;
use quick_xml::events::{BytesStart, Event};

use super::super::resource::Budget;

pub(in crate::nodes::extract) struct XmlDocument {
    depth: u64,
    roots: u64,
    part: &'static str,
}

impl XmlDocument {
    pub(in crate::nodes::extract) fn new(part: &'static str) -> Self {
        Self {
            depth: 0,
            roots: 0,
            part,
        }
    }

    pub(in crate::nodes::extract) fn observe(
        &mut self,
        event: &Event<'_>,
        budget: &mut Budget<'_>,
    ) -> Result<()> {
        match event {
            Event::Start(start) => {
                visit_attributes(start, self.part, budget, |_, _, _| Ok(()))?;
                if self.depth == 0 {
                    self.observe_root()?;
                }
                self.depth = self.depth.checked_add(1).ok_or_else(|| {
                    anyhow::anyhow!("extract_word: XML nesting is too deep in {}", self.part)
                })?;
            }
            Event::Empty(start) => {
                visit_attributes(start, self.part, budget, |_, _, _| Ok(()))?;
                if self.depth == 0 {
                    self.observe_root()?;
                }
            }
            Event::End(_) => {
                self.depth = self.depth.checked_sub(1).ok_or_else(|| {
                    anyhow::anyhow!("extract_word: invalid {}: unmatched closing tag", self.part)
                })?;
            }
            Event::Eof if self.depth != 0 => {
                anyhow::bail!(
                    "extract_word: invalid {}: document ended with {} unclosed element(s)",
                    self.part,
                    self.depth
                );
            }
            Event::Eof if self.roots != 1 => {
                anyhow::bail!(
                    "extract_word: invalid {}: expected one root element, found {}",
                    self.part,
                    self.roots
                );
            }
            Event::Text(text)
                if self.depth == 0 && text.iter().any(|byte| !byte.is_ascii_whitespace()) =>
            {
                anyhow::bail!(
                    "extract_word: invalid {}: text is not allowed outside the root element",
                    self.part
                );
            }
            Event::CData(data) if self.depth == 0 && !data.is_empty() => {
                anyhow::bail!(
                    "extract_word: invalid {}: CDATA is not allowed outside the root element",
                    self.part
                );
            }
            Event::GeneralRef(_) if self.depth == 0 => {
                anyhow::bail!(
                    "extract_word: invalid {}: entity reference is not allowed outside the root element",
                    self.part
                );
            }
            Event::DocType(_) => {
                anyhow::bail!("extract_word: DTDs are not supported in {}", self.part);
            }
            _ => {}
        }
        Ok(())
    }

    fn observe_root(&mut self) -> Result<()> {
        self.roots = self.roots.saturating_add(1);
        if self.roots > 1 {
            anyhow::bail!(
                "extract_word: invalid {}: multiple root elements are not supported",
                self.part
            );
        }
        Ok(())
    }
}

pub(in crate::nodes::extract) fn visit_attributes(
    event: &BytesStart<'_>,
    part: &str,
    budget: &mut Budget<'_>,
    mut visit: impl FnMut(&[u8], &[u8], &mut Budget<'_>) -> Result<()>,
) -> Result<()> {
    for attribute in event.attributes() {
        budget.charge_item("DOCX XML attributes")?;
        let attribute = attribute.map_err(|error| {
            anyhow::anyhow!("extract_word: invalid attribute in {part}: {error}")
        })?;
        visit(attribute.key.as_ref(), attribute.value.as_ref(), budget)?;
    }
    Ok(())
}

pub(super) fn attribute_value(
    event: &BytesStart<'_>,
    name: &[u8],
    part: &str,
    budget: &mut Budget<'_>,
) -> Result<Option<String>> {
    let mut value = None;
    visit_attributes(event, part, budget, |key, raw, _| {
        if key == name {
            value = Some(String::from_utf8_lossy(raw).to_string());
        }
        Ok(())
    })?;
    Ok(value)
}
