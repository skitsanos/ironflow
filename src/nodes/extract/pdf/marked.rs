use super::{super::resource::Budget, resolve, text_output::Output};
use anyhow::{Context as _, Result, ensure};
use lopdf::{Document, ObjectId, content::Operation};

#[derive(Default)]
pub(super) struct Marked {
    stack: Vec<bool>,
    replacements: usize,
}

impl Marked {
    pub(super) fn hidden(&self) -> bool {
        self.replacements > 0
    }

    pub(super) fn process(
        &mut self,
        doc: &Document,
        page: ObjectId,
        operation: &Operation,
        output: &mut Output,
        budget: &mut Budget<'_>,
    ) -> Result<bool> {
        match operation.operator.as_str() {
            "BMC" | "BDC" => {
                ensure!(self.stack.len() < 64, "PDF marked-content depth exceeds 64");
                let operands = &operation.operands;
                ensure!(
                    operands.len() == if operation.operator == "BDC" { 2 } else { 1 },
                    "invalid PDF marked-content operand count"
                );
                operands[0]
                    .as_name()
                    .context("PDF marked-content tag is not a name")?;
                let mut actual = false;
                if operation.operator == "BDC" {
                    let properties = resolve::properties(doc, page, &operands[1], budget)?;
                    if let Ok(value) = properties.get(b"ActualText") {
                        let value = resolve::object(doc, value, budget)?;
                        output.actual(value, self.hidden(), budget)?;
                        actual = true;
                    }
                }
                self.stack.push(actual);
                if actual {
                    self.replacements += 1;
                }
            }
            "EMC" => {
                ensure!(operation.operands.is_empty(), "invalid PDF EMC operands");
                let actual = self
                    .stack
                    .pop()
                    .context("unbalanced PDF marked-content end")?;
                if actual {
                    self.replacements -= 1;
                }
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    pub(super) fn finish(&self) -> Result<()> {
        ensure!(
            self.stack.is_empty(),
            "unclosed PDF marked-content sequence"
        );
        Ok(())
    }
}
